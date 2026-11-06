# v0.9.2 benchmark slice — S-PLG-01 on netcup and Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 benchmark scenarios executed; no scenario remains. Roadmap item
16 stays open for the pending profiling/decisions of the already recorded
Worse results.** S-PLG-01 was executed on both hosts with **5 repetitions per
arm**, **2 GiB and 2 CPU per proxy**, 64 keep-alive HTTP/1.1 connections and a
1 KiB response. The only difference between arms is
`[listener.web.plugin]`; the plugin adds `x-plugin: on`.

The result is a **Measured delta**, not a Better/Worse comparison against
another product: the plugin costs roughly half of the throughput and raises
latency. It is a real, bounded A/B on the same EQProxy image, origin and host.

No smoke or preflight is used as evidence. The published data are the 20
measured series (5 per arm per host) plus one direct-origin calibration per
host.

## 1. Scenario and method

S-PLG-01 is the “with and without plugins loaded” scenario from
`BENCHMARK_PLAN.md` §3. The measured target is the change in RPS, latency and
CPU caused by a real WASM request filter; there is no second proxy product in
this scenario.

- **Generator:** `oha 1.16.0`, HTTP/1.1, **64 keep-alive connections**, one
  request at a time per worker, 1 KiB body. The 1.16 binary is
  `/root/bin/oha-1.16.0` on netcup and `$HOME/bin/oha` on Oracle.
- **Origin:** real nginx (`nginx:alpine`) serving a 1024-byte file, with an
  access log that records `$request`, `$status` and `$http_x_plugin`.
- **Proxy:** one EQProxy container at a time, **2 GiB / 2 CPUs**, with the
  published loopback port only for the generator. No TLS, rules, compression
  or other filters.
- **Duration:** 3 s warm-up + **15 s measured**; 5 repetitions per arm,
  alternating order. Every measured request used the same target and origin.
- **Plugin:** `eqproxy-net/tests/fixtures/plugins/add_header.wasm`, SHA-256
  `0a80ff90117d9a9ed045edeabb99e263ee27ad0db0153d7c1c0327bf003ee46e`.
  `timeout = "500ms"` (not the 50 ms default) so a scheduler pause is not
  mistaken for a plugin time limit; fuel 10 M, 16 MiB, 16 header ops,
  `on_error = "closed"`.
- **Plugin proof:** the nginx access log of each plugin run counts lines whose
  request is `GET / HTTP/1.1`, status 200 and `x-plugin` is `on`. Every plugin
  run must have `origin_plugin_seen == successful responses`; every no-plugin
  run must have zero. This is the check that distinguishes “configured” from
  “executed”.
- **CPU:** cgroup v2 `cpu.stat` `usage_usec`, read immediately before and after
  the measured generator run. The percentage is relative to one CPU, while the
  container limit is two CPUs; 200 % means both CPUs are saturated.
- **Memory:** cgroup `memory.peak` (fallback to VmRSS if absent), compared with
  the 2 GiB limit.
- **Calibration:** direct nginx origin through the same `oha` binary and
  connection count, 3 s warm-up + 5 s measured. It must exceed **1.20×** the
  faster proxy arm.

### Why `oha` and nginx instead of the earlier Go generator

The Go generator (`scripts/bench_v092_plg_load`) and its 8 tests are a valid
local harness, and it completed a Netcup pilot. Its arm64 binary could not be
transferred intact to Oracle: repeated `scp`/`rsync` attempts stalled and the
temporary file never reached the expected SHA-256. That pilot is **not**
published as evidence. The authoritative campaign uses the same `oha 1.16.0`
and the same nginx origin on both hosts, so the within-host A/B remains
controlled.

## 2. Results

Medians of 5 runs per arm. `errors` includes non-200 statuses and generator
errors; all runs had zero.

### netcup-bench (amd64)

| Arm | RPS | p50 | p95 | p99 | CPU (1 CPU = 100 %) | Peak RSS | Errors | Header seen |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| without plugin | **58,002.2** | **1.069 ms** | **1.552 ms** | **1.942 ms** | **194.2 %** | 15.49 MiB | 0 | 0 |
| with plugin | **24,374.2** | **2.581 ms** | **3.691 ms** | **4.188 ms** | **188.2 %** | 19.44 MiB | 0 | **368,192** |
| Δ | **−57.98 %** | **+141.4 %** | **+137.9 %** | **+115.6 %** | **−6.0 pp** | +3.95 MiB | 0 | — |

- Direct-origin calibration: **99,377.8 rps** (headroom **1.713×** over the
  no-plugin arm).
- The proxy was at the 2-CPU ceiling in both arms. The plugin arm does not use
  less work per request; it completes fewer requests while both arms consume
  approximately the whole CPU allowance. The negative CPU delta is therefore
  **not** a saving.

### oracle-quasar (arm64)

| Arm | RPS | p50 | p95 | p99 | CPU (1 CPU = 100 %) | Peak RSS | Errors | Header seen |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| without plugin | **15,818.5** | **3.848 ms** | **6.681 ms** | **7.990 ms** | **114.0 %** | 13.09 MiB | 0 | 0 |
| with plugin | **7,163.9** | **8.879 ms** | **12.229 ms** | **13.728 ms** | **152.6 %** | 17.50 MiB | 0 | **107,840** |
| Δ | **−54.71 %** | **+130.8 %** | **+83.0 %** | **+71.8 %** | **+38.7 pp** | +4.41 MiB | 0 | — |

- Direct-origin calibration: **32,692.3 rps** (headroom **2.067×** over the
  no-plugin arm).
- The Oracle proxy is not CPU-saturated without the plugin; the plugin raises
  CPU by **38.7 percentage points** while reducing throughput by more than
  half. That is the clearest cost signal in the slice.

## 3. Reading

- The plugin costs **58.0 % of the successful RPS on netcup** and **54.7 % on
  Oracle** in this setup. The direction is the same on both architectures; the
  magnitude differs.
- The p99 penalty is **+115.6 % on netcup** and **+71.8 % on Oracle**.
- The plugin is memory-positive in both hosts (+3.95 MiB and +4.41 MiB peak),
  small in absolute terms and well below the 2 GiB limit.
- No request failed: **0 errors in 20/20 runs**. The header was observed at the
  origin for **100 % of successful plugin responses**; no header was observed
  without the plugin.
- This is **not** a universal plugin tax. It is a per-request WASM instance
  (`Store`) with a trivial header mutation, on one host per architecture, with
  the generator co-located. The result is enough to say the cost is material
  and must be budgeted; it does not identify the internal instruction or
  allocator share.
- **Not Better/Worse**, because there is no second product or alternative
  implementation in S-PLG-01. The applicable classification is **Measured
  delta**.

## 4. Validity

All 20 measured series are valid:

- 5 repetitions per arm per host; alternating order.
- Every run had successful responses and **zero** errors.
- Every plugin run had exactly `origin_plugin_seen == successful responses`;
  every no-plugin run had zero.
- Every proxy stayed running and below the 2 GiB limit.
- Both calibrations exceed the 1.20× gate.
- Total measured requests: **4,418,235** netcup no-plugin, **1,845,578** netcup
  plugin, **1,203,226** Oracle no-plugin and **559,883** Oracle plugin.

## 5. Limits and NOT EXECUTED

1. **Closed-loop generator, no coordinated-omission correction.** Latency is
   measured per completed request; the fixed 64-worker load can understate
   queueing under saturation.
2. **One host per architecture**, generator co-located with the proxy. No CPU
   affinity, separate generator host or NIC controls.
3. **CPU is cgroup accounting, not `perf`.** `perf` is unavailable on the
   evidence hosts (`perf_event_paranoid=4`); no instruction-level cause is
   claimed.
4. **The plugin is intentionally trivial**: one `set_request_header` call.
   Response filters, body access and heavier plugins are not covered.
5. **No TLS, rules, rewrites, compression or caching** in this scenario.
6. **No smoke is evidence.** The short validation runs are not published.
7. **The Go pilot is not evidence.** Its arm64 transfer to Oracle failed before
   a verified binary existed; the published campaign uses `oha`/nginx.
8. **Veyra Proof, Rust Audit and CodeGraph: NOT EXECUTED** for this increment:
   no Rust product code changed; only harnesses and evidence were added.
   Docker Scout and a release image were not used.
9. **Roadmap item 16 is not closed.** All 18 scenarios now have executed
   evidence, but the required profiling/decisions for previously recorded
   *Worse* results remain pending.

## 6. Reproduction

```bash
# netcup (amd64)
python3 scripts/bench_v092_plg_oha.py \
  --out /root/bench-v092-plg-oha \
  --oha /root/bin/oha-1.16.0 \
  --eq-image eqproxy-bench:v092 \
  --reps 5 --conns 64 --warmup 3 --duration 15

# Oracle (arm64)
python3 scripts/bench_v092_plg_oha.py \
  --out "$HOME/bench-v092-plg-oha" \
  --oha "$HOME/bin/oha" \
  --eq-image eqproxy-bench:v092 \
  --reps 5 --conns 64 --warmup 3 --duration 15
```

The per-host `checksums.sha256` covers every published file in that host. The
root `checksums.sha256` covers both host trees and this README. The two
per-host manifests verify **98/98**; the root manifest has **199** entries and
includes the nested per-host checksum files, as in the earlier slices.
