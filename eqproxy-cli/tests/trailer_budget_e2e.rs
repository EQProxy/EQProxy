//! AUD-03 release journey through the real `eqproxy` binary.
//!
//! A real chunked origin sends a small compressed-eligible body followed by
//! 100 000 trailers of ~7.9 KiB. The proxy must stop at the shared trailer
//! budget and keep the process RSS bounded instead of accumulating the
//! section. The measurement is taken from `ps` on the live child process and
//! both the baseline and peak are printed for the evidence log.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TRAILERS: usize = 100_000;
const PADDING: usize = 8 * 1024 - 64;
const RSS_BUDGET_KIB: u64 = 64 * 1024;

fn temp_config(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "eqproxy-trailer-budget-{}-{name}.toml",
        std::process::id()
    ));
    std::fs::write(&path, contents).expect("write test config");
    path
}

fn remove_temp(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn rss_kib(pid: u32) -> u64 {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("run ps for RSS");
    assert!(
        output.status.success(),
        "ps failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .expect("parse RSS in KiB")
}

fn start_trailer_origin() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind trailer origin");
    let addr = listener.local_addr().expect("trailer origin addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            std::thread::spawn(move || {
                let mut head = [0u8; 4096];
                let mut total = 0usize;
                loop {
                    match stream.read(&mut head[total..]) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => total += n,
                    }
                    if head[..total].windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                    if total == head.len() {
                        return;
                    }
                }
                if stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                          Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
                          4\r\nbody\r\n0\r\n",
                    )
                    .is_err()
                {
                    return;
                }
                let line = format!("x-trailer: {}\r\n", "x".repeat(PADDING));
                for _ in 0..TRAILERS {
                    if stream.write_all(line.as_bytes()).is_err() {
                        return;
                    }
                }
                let _ = stream.write_all(b"\r\n");
            });
        }
    });
    addr
}

fn stop_child(child: &mut Child) {
    let pid = child.id();
    let _ = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(_status) = child.try_wait().expect("try_wait") {
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("eqproxy did not finish after SIGINT");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg_attr(
    miri,
    ignore = "needs processes and TCP sockets: miri does not support them (rust-lang/miri#3449)"
)]
#[test]
fn binary_keeps_rss_bounded_on_a_huge_compressed_trailer_section() {
    let origin = start_trailer_origin();
    let config = temp_config(
        "rss",
        &format!(
            "schema_version = 1\n\
             [listener.web]\n\
             listen = \"127.0.0.1:0\"\n\
             mode = \"http\"\n\
             backend = \"api\"\n\
             [listener.web.compression]\n\
             gzip = true\n\
             min_size = 0\n\
             types = [\"text/plain\"]\n\
             [backend.api]\n\
             server = \"{origin}\"\n"
        ),
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_eqproxy"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--metrics-interval", "1h"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn eqproxy run");
    let mut stderr = BufReader::new(child.stderr.take().expect("stderr piped"));

    let mut proxy = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        let read = stderr.read_line(&mut line).expect("read stderr");
        assert!(read > 0, "eqproxy closed stderr without announcing bind");
        if let Some(addr) = line.split("listening on ").nth(1) {
            proxy = addr.trim().to_owned();
            break;
        }
    }
    assert!(!proxy.is_empty(), "listener address was not announced");

    let baseline = rss_kib(child.id());
    assert!(baseline > 0, "baseline RSS must be real");

    let mut client = TcpStream::connect(proxy.as_str()).expect("connect proxy");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    client
        .write_all(
            b"GET /trailers HTTP/1.1\r\nHost: t\r\nAccept-Encoding: gzip\r\n\
              Connection: close\r\n\r\n",
        )
        .expect("write GET");

    let mut response = Vec::new();
    let started = Instant::now();
    let _ = client.read_to_end(&mut response);
    let elapsed = started.elapsed();

    // Keep the child alive while sampling: a cut connection can be observed
    // slightly before the process finishes its bookkeeping.
    std::thread::sleep(Duration::from_millis(100));
    let peak = rss_kib(child.id());
    println!(
        "TRAILER_RSS baseline_kib={baseline} peak_kib={peak} growth_kib={} budget_kib={RSS_BUDGET_KIB} response_bytes={} elapsed_ms={}",
        peak.saturating_sub(baseline),
        response.len(),
        elapsed.as_millis()
    );
    assert!(
        peak.saturating_sub(baseline) <= RSS_BUDGET_KIB,
        "RSS grew by {} KiB, over the declared {RSS_BUDGET_KIB} KiB budget",
        peak.saturating_sub(baseline)
    );
    assert!(
        !response.is_empty(),
        "the client must receive the committed response head, not an empty close"
    );

    stop_child(&mut child);
    remove_temp(&config);
}
