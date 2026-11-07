//! Optional CPU/memory profiling for the benchmark evidence hosts (feature
//! `profile`).
//!
//! The hosts used for the v0.9.2 campaigns report
//! `kernel.perf_event_paranoid=4`, so `perf` and every `perf_event_open`
//! consumer (including `bpftrace`'s profile probe) are unavailable. `pprof`
//! samples with a `setitimer(ITIMER_PROF)` and a `SIGPROF` handler instead,
//! which is allowed at that level: the process samples itself, no kernel
//! counter is opened.
//!
//! Two streams go to stderr, both delimited so a harness can extract them
//! from `docker logs`:
//!
//! - `PPROF_SAMPLE {...}` every 500 ms: RSS/HWM, threads, context switches,
//!   syscall counts and bytes, scheduler run/wait time and socket counts.
//!   These are the substitute for `perf stat` and for a syscall profiler:
//!   every value is a real `/proc/self` reading, not an estimate.
//! - `PPROF_SVG_BEGIN` / `PPROF_SVG_END` once, after the proxy has drained
//!   and just before exit: the flamegraph of the whole invocation. The SVG
//!   is rendered with inferno 0.12 directly (ADR-0049), not through
//!   `pprof`'s `flamegraph` feature, which pins the vulnerable quick-xml
//!   0.26.
//!
//! This module is compiled only with the non-default `profile` feature and
//! is inert unless `EQPROXY_PPROF_DIR` is set. The release image does not
//! enable the feature, so the production binary carries neither the sampler
//! nor its dependency tree.
//!
//! The dump happens on the normal shutdown path (`main` returns after
//! `run`). The container must therefore be stopped with SIGINT or SIGTERM
//! and enough grace, not SIGKILL, or the flamegraph is lost.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A running sampler. Kept alive for the whole process lifetime, because
/// dropping the pprof guard stops sampling.
pub struct Profiler {
    guard: pprof::ProfilerGuard<'static>,
    dir: PathBuf,
    stop: Arc<AtomicBool>,
    sampler: Option<std::thread::JoinHandle<()>>,
}

/// Starts sampling when `EQPROXY_PPROF_DIR` names a directory.
///
/// 99 Hz is the conventional non-round rate: it avoids locking the sampler
/// to the same cadence as periodic work (metrics, health checks) and gives
/// ~1 500 samples over a 15 s campaign, enough for a stable flamegraph.
pub fn start() -> Option<Profiler> {
    let dir = std::env::var_os("EQPROXY_PPROF_DIR")?;
    if dir.is_empty() {
        return None;
    }
    let guard = match pprof::ProfilerGuardBuilder::default()
        .frequency(99)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
    {
        Ok(guard) => guard,
        Err(err) => {
            // Profiling must never take the proxy down or pretend it worked.
            eprintln!("warning: pprof sampler could not start: {err}");
            return None;
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = if std::env::var_os("EQPROXY_PROFILE_SAMPLES").is_some_and(|v| v == "1") {
        let stop = Arc::clone(&stop);
        Some(std::thread::spawn(move || sample_loop(&stop)))
    } else {
        None
    };
    eprintln!("INFO pprof sampler started at 99 Hz; the flamegraph is written on shutdown");
    Some(Profiler {
        guard,
        dir: PathBuf::from(dir),
        stop,
        sampler,
    })
}

/// One line per sample. Kept as a single `eprintln!` so concurrent proxy
/// logging cannot interleave inside a record (a line is written atomically
/// to stderr up to the pipe buffer size).
fn sample_loop(stop: &AtomicBool) {
    let started = Instant::now();
    let mut tick = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let at_ms = started.elapsed().as_millis();
        let status = read_key_values("/proc/self/status");
        let io = read_key_values("/proc/self/io");
        let sched = std::fs::read_to_string("/proc/self/schedstat").unwrap_or_default();
        let mut sched_fields = sched.split_whitespace();
        let run_ns = sched_fields.next().and_then(|v| v.parse::<u64>().ok());
        let wait_ns = sched_fields.next().and_then(|v| v.parse::<u64>().ok());
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
        let stat_fields: Vec<&str> = stat.split_whitespace().collect();
        let utime = stat_fields.get(13).and_then(|v| v.parse::<u64>().ok());
        let stime = stat_fields.get(14).and_then(|v| v.parse::<u64>().ok());
        let threads = stat_fields.get(19).and_then(|v| v.parse::<u64>().ok());
        let cgroup_current = read_u64("/sys/fs/cgroup/memory.current");
        let cgroup_peak = read_u64("/sys/fs/cgroup/memory.peak");
        let fds = std::fs::read_dir("/proc/self/fd")
            .map(|entries| entries.count())
            .unwrap_or(0);
        let sockets = std::fs::read_to_string("/proc/self/net/tcp")
            .map(|text| text.lines().count().saturating_sub(1))
            .unwrap_or(0);
        // smaps_rollup walks the VMAs and is comparatively expensive; the
        // cheap /proc/self/status above is sampled every tick, the detailed
        // anonymous/private breakdown every fourth tick (2 s).
        let (anon_kb, private_dirty_kb) = if tick.is_multiple_of(4) {
            smaps_rollup()
        } else {
            (None, None)
        };
        tick += 1;
        let json = format!(
            "{{\"t_ms\":{at_ms},\"rss_kb\":{},\"hwm_kb\":{},\"vm_size_kb\":{},\
             \"threads_status\":{},\"threads_stat\":{},\"voluntary_ctxt\":{},\
             \"involuntary_ctxt\":{},\"syscr\":{},\"syscw\":{},\"rchar\":{},\
             \"wchar\":{},\"run_ns\":{},\"wait_ns\":{},\"utime\":{},\"stime\":{},\
             \"cg_current\":{},\"cg_peak\":{},\"fds\":{},\"tcp_sockets\":{},\
             \"anon_kb\":{},\"private_dirty_kb\":{}}}",
            key(&status, "VmRSS"),
            key(&status, "VmHWM"),
            key(&status, "VmSize"),
            key(&status, "Threads"),
            opt(threads),
            key(&status, "voluntary_ctxt_switches"),
            key(&status, "nonvoluntary_ctxt_switches"),
            key(&io, "syscr"),
            key(&io, "syscw"),
            key(&io, "rchar"),
            key(&io, "wchar"),
            opt(run_ns),
            opt(wait_ns),
            opt(utime),
            opt(stime),
            opt(cgroup_current),
            opt(cgroup_peak),
            fds,
            sockets,
            opt(anon_kb),
            opt(private_dirty_kb),
        );
        eprintln!("PPROF_SAMPLE {json}");
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn read_key_values(path: &str) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    Some((
                        name.trim().to_owned(),
                        value
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn key(values: &[(String, String)], name: &str) -> String {
    values
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| "0".to_owned())
}

fn read_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

fn opt(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn smaps_rollup() -> (Option<u64>, Option<u64>) {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let mut anon = None;
    let mut dirty = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Anonymous:") {
            anon = value.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(value) = line.strip_prefix("Private_Dirty:") {
            dirty = value.split_whitespace().next().and_then(|v| v.parse().ok());
        }
    }
    (anon, dirty)
}

/// Renders the folded stacks with inferno 0.12.
///
/// `pprof`'s `flamegraph` feature pins inferno 0.11, whose `quick-xml 0.26`
/// carries RUSTSEC-2026-0194/0195. The folded-stack format is the one
/// `pprof` uses; only the renderer moves to inferno 0.12 (ADR-0049).
///
/// Returns `Ok(false)` when the report has no samples: no SVG is produced,
/// so `finish` can report the empty report instead of claiming a dump. The
/// first 20-request SIGTERM run produced exactly this case and left a
/// zero-byte SVG; the harness correctly rejected it.
fn write_flamegraph<W: std::io::Write>(report: &pprof::Report, writer: W) -> Result<bool, String> {
    let lines: Vec<String> = report
        .data
        .iter()
        .map(|(frames, count)| {
            let mut line = frames.thread_name_or_id();
            line.push(';');
            for frame in frames.frames.iter().rev() {
                for symbol in frame.iter().rev() {
                    line.push_str(&symbol.name());
                    line.push(';');
                }
            }
            line.pop();
            let _ = write!(&mut line, " {count}");
            line
        })
        .collect();
    if lines.is_empty() {
        return Ok(false);
    }
    let mut options = inferno::flamegraph::Options::default();
    inferno::flamegraph::from_lines(&mut options, lines.iter().map(String::as_str), writer)
        .map_err(|err| err.to_string())?;
    Ok(true)
}

/// Writes the flamegraph (and a textual stack counter) and stops sampling.
///
/// A failure or an empty report is reported, never converted into a
/// successful measurement: the harness records whether the SVG exists and
/// treats its absence as NOT EXECUTED for that run. An empty report does not
/// create a zero-byte SVG and does not emit the `PPROF_SVG_*` markers.
pub fn finish(profiler: Option<Profiler>) {
    let Some(profiler) = profiler else {
        return;
    };
    profiler.stop.store(true, Ordering::Relaxed);
    if let Some(sampler) = profiler.sampler {
        let _ = sampler.join();
    }
    if let Err(err) = std::fs::create_dir_all(&profiler.dir) {
        eprintln!(
            "warning: cannot create pprof directory {}: {err}",
            profiler.dir.display()
        );
        return;
    }
    let stem = format!("eqproxy-pprof-{}", std::process::id());
    let report = match profiler.guard.report().build() {
        Ok(report) => report,
        Err(err) => {
            eprintln!("warning: pprof could not build a report: {err}");
            return;
        }
    };
    let svg = profiler.dir.join(format!("{stem}.svg"));
    let mut svg_bytes = Vec::new();
    match write_flamegraph(&report, &mut svg_bytes) {
        Ok(true) => match std::fs::write(&svg, &svg_bytes) {
            Ok(()) => eprintln!("INFO pprof flamegraph written to {}", svg.display()),
            Err(err) => eprintln!("warning: pprof could not write {}: {err}", svg.display()),
        },
        Ok(false) => eprintln!("warning: pprof report has no samples; no flamegraph written"),
        Err(err) => eprintln!("warning: pprof could not render {}: {err}", svg.display()),
    }
    // The text report is the same sample counts in Debug form; it lets the
    // analysis attribute self time without parsing SVG.
    let txt = profiler.dir.join(format!("{stem}.txt"));
    match File::create(&txt) {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{report:?}") {
                eprintln!("warning: pprof could not write {}: {err}", txt.display());
            }
        }
        Err(err) => eprintln!(
            "warning: cannot create pprof text report {}: {err}",
            txt.display()
        ),
    }
    // Also emit the SVG to stderr: the harnesses save `docker logs` before
    // removing the container, so the dump survives even when the container
    // filesystem does not. The markers keep it extractable from the log.
    if !svg_bytes.is_empty() {
        eprintln!("PPROF_SVG_BEGIN");
        let _ = std::io::stderr().write_all(&svg_bytes);
        eprintln!("\nPPROF_SVG_END");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::SystemTime;

    #[test]
    fn flamegraph_svg_contains_the_sampled_frames() {
        let frames = pprof::Frames {
            frames: vec![vec![pprof::Symbol {
                name: Some(b"main".to_vec()),
                addr: None,
                lineno: None,
                filename: None,
            }]],
            thread_name: "worker".to_owned(),
            thread_id: 1,
            sample_timestamp: SystemTime::UNIX_EPOCH,
        };
        let report = pprof::Report {
            data: HashMap::from([(frames, 3)]),
            timing: Default::default(),
        };
        let mut svg = Vec::new();
        assert!(write_flamegraph(&report, &mut svg).expect("flamegraph"));
        let svg = String::from_utf8(svg).expect("utf8 svg");
        assert!(svg.contains("<svg"), "must be an SVG document");
        assert!(svg.contains("main"), "the sampled frame must be present");
    }

    #[test]
    fn empty_report_is_not_rendered() {
        let report = pprof::Report {
            data: HashMap::new(),
            timing: Default::default(),
        };
        let mut svg = Vec::new();
        assert_eq!(write_flamegraph(&report, &mut svg), Ok(false));
        assert!(svg.is_empty(), "an empty report must not produce an SVG");
    }
}
