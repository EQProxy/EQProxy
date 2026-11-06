# v0.9.2 benchmark slice — S-OVL-01 on netcup and Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 scenarios executed; no benchmark scenario remains; item 16 remains open for profiling/decisions.** S-OVL-01 was executed on
both hosts with **5 repetitions per product**, **2 GiB and 2 CPU per proxy**,
**64 closed-loop workers against `maxconn = 16`**, a 100 ms origin and a 15 s
measured window. The declared result is **Not comparable**: EQProxy refuses
the excess connection at accept time, while HAProxy leaves it in the kernel
accept queue. Both policies are valid, but they do not produce the same client
outcome, so comparing their error counts as a performance score would be
dishonest.

No smoke or preflight run is used as evidence. The published data are the
10 measured series per host (5 per product) plus the direct-origin calibration.

## 1. Scenario and method

S-OVL-01 is the sustained-overload scenario from `BENCHMARK_PLAN.md` §3
(A18/T08): more concurrent clients than the proxy's connection cap, with
observable errors, memory and queues.

- **Generator:** `scripts/bench_v092_ovl_load` (Go), HTTP/1.1, **one fresh
  connection per request**, 64 workers, 1 KiB response body.
- **Origin:** real HTTP/1.1 server with a **100 ms service delay** (`/healthz`
  is not delayed).
- **Duration:** 3 s warm-up + **15 s measured**; 5 repetitions, alternating
  product order. The measured window is entirely above the cap: 64 workers
  against 16 connection slots (4× offered concurrency).
- **Proxies:** one at a time, in Docker, **2 GiB / 2 CPUs**, no host port
  published. Origin and generator are outside those limits.
- **Connection cap:** `maxconn = 16` in both: EQProxy listener `maxconn` and
  HAProxy frontend `maxconn`; HAProxy also has `server s1 … maxconn 16`.
- **EQProxy policy:** when the listener is full, the accepted socket is closed
  immediately and `rejected_maxconn` is incremented; there is no queue.
- **HAProxy policy:** when the frontend cap is full, HAProxy stops accepting;
  excess TCP connections wait in the kernel accept queue. HAProxy's own
  `qmax` is not the relevant queue for this path (the queue is below HAProxy),
  so the harness samples the listening socket's `rx_queue` from
  `/proc/net/tcp` inside the proxy container for both products.
- **Memory:** cgroup `memory.current` sampled every 250 ms, with
  `memory.peak` used at the end when available. The 2 GiB cgroup limit is
  recorded in every run.
- **Calibration:** direct-origin run with the same 64 workers and 100 ms
  delay, 3 s warm-up + 5 s measured. It must reach **1.20× the faster proxy's
  successful RPS**; a run with rejected connections does not count as
  throughput.

## 2. netcup — 8 vCPU, 64 workers

- Architecture `x86_64`; Linux 6.8; Docker 29.6.1.
- EQProxy image `a461b0f678128017`; HAProxy `e273915691a8eab3`.
- Calibration: **627,2 successful rps**, 0 errors; headroom **3,973×**; valid.
- **10/10 runs valid**; **27/27 checksums**.

| Product | Success RPS | Errors | Error rate | p50 success | p95 success | p99 success | Active / smax | Rejected | Listen queue max | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 156,8 | 565.801 | **99,586 %** | 103,2 ms | 106,0 ms | 107,7 ms | **16** | **665.984** | 45 | 13,5 MiB |
| HAProxy | 157,9 | 0 | **0 %** | 306,1 ms | 812,8 ms | **1.421,8 ms** | **16** | — | 15 | 24,6 MiB |

Ranges over the five runs:

| Product | Success RPS | Errors | p99 success | Peak RSS |
|---|---:|---:|---:|---:|
| EQProxy | 156,0–156,8 | 551.761–673.135 | 107,02–108,44 ms | 13,4–13,9 MiB |
| HAProxy | 157,9 | 0 | 1.012,6–1.621,9 ms | 24,4–24,7 MiB |

- EQProxy error classes were mostly `eof` (≈ 401k median) and
  `connection_reset` (≈ 154k), plus ≈ 0.9k connect errors; all are the
  explicit refusal path, not a crash.
- HAProxy returned no HTTP errors and served all 2.368 requests in the median
  run. Its p99 includes the kernel queue wait.
- Both stayed inside the 2 GiB cgroup limit; no OOM or restart was observed.

## 3. Oracle — 4 vCPU, 64 workers

- Architecture `aarch64`; Linux 7.0; Docker 29.1.3.
- EQProxy image `5a2e16f8b4d219ef`; HAProxy `e273915691a8eab3`.
- Calibration: **627,2 successful rps**, 0 errors; headroom **3,986×**; valid.
- **10/10 runs valid**; **27/27 checksums**.

| Product | Success RPS | Errors | Error rate | p50 success | p95 success | p99 success | Active / smax | Rejected | Listen queue max | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 154,6 | 190.239 | **98,797 %** | 106,5 ms | 112,7 ms | 117,0 ms | **16** | **230.184** | 44 | 12,6 MiB |
| HAProxy | 157,3 | 51 | **2,10 %** | 203,4 ms | 261,1 ms | **2.163,2 ms** | **16** | — | 17 | 20,4 MiB |

Ranges over the five runs:

| Product | Success RPS | Errors | p99 success | Peak RSS |
|---|---:|---:|---:|---:|
| EQProxy | 153,6–154,9 | 183.768–193.966 | 115,76–120,60 ms | 12,2–14,2 MiB |
| HAProxy | 151,9–158,2 | 41–97 | 1.268,8–2.322,9 ms | 20,4–21,4 MiB |

- Oracle HAProxy's 51 errors are all **dial timeouts**: the kernel accept queue
  delayed connection establishment beyond the generator's 10 s client
  timeout. They are not HTTP 5xx responses and they are reported as part of
  the overload result.
- EQProxy's rejections are immediate closes; its p99 is therefore the service
  latency of the accepted requests, not of the rejected ones.
- Both stayed inside the 2 GiB cgroup limit; no OOM or restart was observed.

## 4. Verdict

**Not comparable** on both hosts. The successful throughput is almost the
same (EQProxy 154,6–156,8 rps; HAProxy 157,3–157,9 rps), but the excess load is
handled by different mechanisms:

- EQProxy **rejects** the excess at accept and answers its accepted clients in
  ≈ 108–117 ms p99; it does not build a request queue.
- HAProxy **queues** the excess below the proxy and returns no HTTP error in
  netcup; the queue shows up as a 1,4–2,2 s p99 on successful requests, and on
  Oracle it produces 51 dial timeouts.

Neither result is a clean win. Calling EQProxy better because it has fewer
successful-request errors (it rejects most clients before they get an answer)
or calling HAProxy better because it returns HTTP 200 (after a multi-second
wait) would both be wrong. The scenario is published as executed, classified
**Not comparable**, and **not certified** as a performance recommendation.

## 5. Raw audit

- Each host has **28 files**: report, run log, 10 raw runs, 1 calibration,
  10 proxy logs, 2 config snapshots, 2 runtime configs and
  `checksums.sha256`.
- `sha256sum -c checksums.sha256` passes **27/27** on both hosts;
  the root manifest covers the README plus 28 files per host (**57/57**).
- Every one of the 10 measured records per host has `analysis.valid: true`.
- The five repetitions of each product are present; no failed or discarded run
  is included.
- The HAProxy `admin.sock` is a live socket and is not a regular published
  file; it is not in the checksum list.
- The one-repetition preflights were discarded and are not in this directory.

## 6. Limitations and NOT EXECUTED

1. **Different overload policies.** EQProxy has no listener queue; HAProxy's
   excess waits in the kernel accept queue. The result is Not comparable by
   design, not a missing measurement.
2. **Closed-loop generator**, no coordinated-omission correction. The
   successful-request latency histograms are published; failed connection
   attempts are reported by class.
3. **One connection per request.** Keep-alive and established-connection
   behavior are outside this harness.
4. **One host per architecture**; origin, proxy and generator share the host.
   No CPU affinity, separate machines or NIC controls.
5. **HAProxy's application `qmax` is not the queue under test.** The kernel
   listening-socket `rx_queue` is sampled from inside the container; it is a
   point-in-time maximum, not a full queue histogram.
6. **Oracle HAProxy has 51 client dial timeouts**; they are data, not a harness
   failure. The client timeout is 10 s and is recorded in every raw run.
7. **No profiling.** `perf` is unavailable (`perf_event_paranoid=4`); no cause
   is attributed to CPU, allocator or scheduler.
8. **No TLS, rules, rewrites, compression or plugins** in this scenario.
9. **Veyra, Rust Audit and CodeGraph: NOT EXECUTED** for this increment: no
   Rust product code changed; only the Go/Python harness and evidence were
   added. Docker Scout and a release image were not used.
10. **No smoke is evidence.** The preflights are not published.

## 7. Reproduction

```bash
# netcup (amd64)
python3 bench_v092_ovl.py --out <dir> --load-binary ./ovl-load-amd64 \
  --reps 5 --conns 64 --maxconn 16 --delay-ms 100 \
  --warmup 3 --duration 15 \
  --calibration-warmup 3 --calibration-duration 5

# Oracle (arm64)
python3 bench_v092_ovl.py --out <dir> --load-binary ./ovl-load-arm64 \
  --reps 5 --conns 64 --maxconn 16 --delay-ms 100 \
  --warmup 3 --duration 15 \
  --calibration-warmup 3 --calibration-duration 5
```

## 8. Integrity

```bash
(cd netcup  && sha256sum -c checksums.sha256)
(cd oracle  && sha256sum -c checksums.sha256)
sha256sum -c checksums.sha256
```
