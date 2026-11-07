# v0.9.2 benchmark slice — S-CFG-01 on netcup and Oracle

Date: 2026-11-03 (UTC)

Status: **18/18 scenarios executed; no benchmark scenario remains; item 16 remains open for profiling/decisions.** S-CFG-01 was executed on both
hosts with **5 repetitions per product**, **2 GiB and 2 CPU per proxy**, a
20 s measured series and a real configuration swap from origin A to origin B.
The declared reload-window classification is **Better on netcup** and
**Comparable on Oracle**, with **zero errors** and **zero EQProxy counter
resets**. It is **not certified as a clean steady-state win**: the post-reload
p99 is worse than the baseline, especially for EQProxy. **S-PLG-01 is executed**;
item 16 remains open for the pending profiling/decisions.

No smoke or preflight run is used as evidence. The published data are the
10 measured series per host (5 per product) plus the direct-origin calibration.

## 1. Scenario and method

The scenario asks for config application under traffic (`BENCHMARK_PLAN.md`
§3, D9, PP-04/PP-05). Both products start with a real HTTP origin A; while
closed-loop traffic is running, the operator atomically replaces the config
with one that points to origin B and sends `SIGHUP`. A new request must return
origin B's marker.

- **Generator:** `scripts/bench_v092_cfg_load` (Go), HTTP/1.1, **one fresh
  connection per request**, 32 workers, 1 KiB response, full latency samples.
- **Phases:** `baseline` before the swap, `reload` from just before the signal
  through a **2 s** window, and `post` after it. The p99 is computed from the
  exact samples in each phase, not from the whole run.
- **Timing:** 3 s warm-up + **20 s measured**; reload at 8 s; the generator
  writes a ready marker at the start of measurement so the control script does
  not guess the clock.
- **Reload:** write `config.toml`/`haproxy.cfg` with A→B, `SIGHUP` the
  container, then probe through the proxy until the body is `b`. The probe is
  inside the 2 s window; `reload_ready_ms` is the observed confirmation time.
- **EQProxy:** in-place dataplane swap; counters are read from the periodic
  `METRICS` lines. The harness requires exactly one applied reload and a
  monotonic `accepted`/`proxied` sequence.
- **HAProxy:** master/worker replacement; counters are read through a local
  `stats socket`. A changed worker PID or a lower counter is recorded as a
  metric reset. This reset is a structural difference, reported separately and
  **not** used to claim a performance win.
- **Limits:** each proxy container has 2 GiB and 2 CPUs; origins and generator
  are outside those limits. Nothing traverses a published host port.
- **Calibration:** the same generator runs against origin A with 2 s warm-up +
  5 s measured. A series is valid only at **≥ 1.20×** the faster proxy's RPS.

### What "Better" means here

The predeclared comparison uses the **reload-window p99 ratio** (`reload p99 /
baseline p99`), with a 10 % relative band. Errors are compared exactly. A
metric reset is a separate observation. This is deliberately narrower than a
claim that the proxy is faster after the reload; the post phase is published
next to it.

## 2. netcup — 8 vCPU, 32 workers

- Architecture `x86_64`; Linux 6.8; Docker 29.6.1.
- EQProxy image `a461b0f678128017`; HAProxy `e273915691a8eab3`.
- Calibration: **33,823.6 rps**, 0 errors; headroom **2.204×**; valid.
- **31 files**; **31/31 checksums**; **10/10 runs valid**.

| Product | Baseline p99 | Reload p99 | Post p99 | Reload/base | Errors | Counter resets | RPS |
|---|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 30.572 ms | 34.970 ms | 48.574 ms | **1.209×** | 0 | **0** | 2,682.8 |
| HAProxy | 5.468 ms | 8.769 ms | 9.572 ms | 1.633× | 0 | **5/5** | 15,346.9 |

- Reload confirmed in **619.6 ms** (EQProxy) and **633.9 ms** (HAProxy).
- EQProxy `reloads=1`, `reload_failures=0` in every valid run.
- Reload-window ratio EQ/HA: **0.740×** → **Better** on the declared metric.
- **Do not read this as a steady-state win:** EQProxy's post p99 is 48.574 ms
  against a 30.572 ms baseline, while HAProxy's is 9.572 ms against 5.468 ms.
  The post phase is outside the declared classification window but is not
  hidden.

## 3. Oracle — 4 vCPU, 32 workers

- Architecture `aarch64`; Linux 7.0; Docker 29.1.3.
- EQProxy image `5a2e16f8b4d219ef`; HAProxy `e273915691a8eab3`.
- Calibration: **11,046.6 rps**, 0 errors; headroom **1.983×**; valid.
- **31 files**; **31/31 checksums**; **10/10 runs valid**.

| Product | Baseline p99 | Reload p99 | Post p99 | Reload/base | Errors | Counter resets | RPS |
|---|---:|---:|---:|---:|---:|---:|---:|
| EQProxy | 11.212 ms | 18.739 ms | 39.592 ms | 1.652× | 0 | **0** | 4,492.4 |
| HAProxy | 12.147 ms | 19.223 ms | 12.447 ms | 1.572× | 0 | **5/5** | 5,572.0 |

- Reload confirmed in **613.8 ms** (EQProxy) and **593.5 ms** (HAProxy).
- EQProxy `reloads=1`, `reload_failures=0` in every valid run.
- Reload-window ratio EQ/HA: **1.051×** → **Comparable** on the declared metric.
- **Post is worse for EQProxy:** 39.592 ms vs the 11.212 ms baseline; HAProxy
  returns to 12.447 ms. The result is not a clean steady-state win.

## 4. Verdict

| Host | Reload p99 EQ/HA | Errors | Metric resets EQ/HA | Declared result |
|---|---:|---:|---:|---|
| netcup | 0.740× | 0 / 0 | 0 / 5 | **Better** (reload window) |
| Oracle | 1.051× | 0 / 0 | 0 / 5 | **Comparable** (reload window) |

The configuration swap itself was applied and observed on both hosts, with
zero HTTP errors. EQProxy kept its counters across the swap; HAProxy's worker
replacement reset its stats in all ten HAProxy runs. **S-CFG-01 is executed,
not certified as a clean win**, because the post-reload p99 is worse than the
baseline in both hosts (most visibly on EQProxy Oracle).

## 5. Raw audit

- Each host has **31 files**: 10 measured JSON records, 1 calibration, 10 proxy
  logs, 4 config snapshots, `report.json`, `run.log`, 2 runtime configs and
  `checksums.sha256`.
- `sha256sum -c checksums.sha256` passes **31/31** on both hosts.
- Every one of the 10 measured records has `analysis.valid: true`.
- The 5 repetitions of each product are present; no failed or discarded run is
  included.
- The config hashes in the records correspond to the post-swap B config saved
  in `configs/`.
- The HAProxy `admin.sock` is a live socket and is not a regular published
  file; it is not included in the checksum list.

## 6. Limitations and NOT EXECUTED

1. **One connection per request.** The swap is visible on every new request,
   but established-connection draining is not measured here; the Rust
   `e2e_reload.rs` journeys cover that separately.
2. **Closed-loop generator**, no coordinated-omission correction. Full
   latency samples and histograms are published, but p99 is not CO-corrected.
3. **One host per architecture**; origin, proxy and generator share it. No CPU
   affinity, separate machines or NIC controls.
4. **Post-reload p99 is worse than baseline**, especially for EQProxy; the
   classification is only the declared 2 s reload window.
5. **HAProxy worker replacement resets its counters** by design; EQProxy's
   in-place swap does not. This is reported as a structural difference, not
   converted into a performance claim.
6. **No profiling.** `perf` is unavailable (`perf_event_paranoid=4`); no cause
   is attributed to CPU, allocator or scheduler.
7. **No TLS, rules, rewrites, compression or plugins** in this scenario.
8. **Veyra, Rust Audit and CodeGraph: NOT EXECUTED** for this increment: no
   Rust code changed; only the Go/Python harness and evidence were added.
   Docker Scout and a release image were also not used.
9. **No smoke is evidence.** The one-repetition preflights were discarded and
   are not in this directory.

## 7. Reproduction

```bash
# netcup (amd64)
python3 bench_v092_cfg.py --out <dir> --load-binary ./cfg-load-amd64 \
  --reps 5 --conns 32 --warmup 3 --duration 20 \
  --reload-at 8 --reload-window 2 \
  --calibration-warmup 2 --calibration-duration 5

# Oracle (arm64)
python3 bench_v092_cfg.py --out <dir> --load-binary ./cfg-load-arm64 \
  --reps 5 --conns 32 --warmup 3 --duration 20 \
  --reload-at 8 --reload-window 2 \
  --calibration-warmup 2 --calibration-duration 5
```

## 8. Integrity

```bash
(cd netcup  && sha256sum -c checksums.sha256)
(cd oracle  && sha256sum -c checksums.sha256)
sha256sum -c checksums.sha256
```
