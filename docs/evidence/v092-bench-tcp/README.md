# v0.9.2 benchmark slice — S-TCP-01/02 on netcup and Oracle

Date: 2026-11-03 (UTC)
Status: **partial campaign; after this slice the roadmap is 18/18 scenarios
executed; no benchmark scenario remains. Item 16 stays open for profiling/decisions. S-TCP-01 and S-TCP-02 have valid raw
evidence, but neither is a clean win on every metric.**

This directory contains the two TCP scenarios from `BENCHMARK_PLAN.md` §3 on
both supported architectures. The earlier H1/H2/TLS slices are in
[`../v092-bench/`](../v092-bench/), [`../v092-bench-h1/`](../v092-bench-h1/),
[`../v092-bench-h2/`](../v092-bench-h2/) and
[`../v092-bench-tls/`](../v092-bench-tls/). The TLS 1.3 session-resumption
change is separate: [`../v092-tls-resumption.md`](../v092-tls-resumption.md).

The Mac was not used for measurements; it only cross-compiled the two Linux
Go binaries. The campaign ran on `netcup-bench` (amd64) and `oracle-quasar`
(arm64), with each **proxy** container limited to **2 GiB and 2 CPUs**. The
origin and the generator were not given those limits, so they could not hide a
proxy ceiling.

```text
benchv092-tcp-eq: 2147483648 bytes  2000000000 NanoCpus
benchv092-tcp-ha: 2147483648 bytes  2000000000 NanoCpus
```

## 1. Design and disclosure

| Field | netcup-bench | oracle-quasar |
|---|---|---|
| Architecture | `x86_64` / amd64 | `aarch64` / arm64 |
| Host | AMD EPYC 9645, 8 vCPU exposed, ~15 GiB, Ubuntu 24.04, Docker 29.6.1 | Oracle Ampere, 4 vCPU, ~23 GiB, Ubuntu 24.04, Docker 29.1.3 |
| Host load1 at start | 0.51 | 1.04 |
| EQProxy image | `eqproxy-bench:v092` / `a461b0f678128017` | `eqproxy-bench:v092` / `5a2e16f8b4d219ef` |
| HAProxy image | `haproxy:3.4.6` / `e273915691a8eab3` | `haproxy:3.4.6` / `e273915691a8eab3` |
| Generator SHA-256 | `2be96c8a4a32f1f6348dba1091f59d9bf60c85416117fc591e5fd909c37db965` | `d815a69add1833a27e1d5ff262e3a5f1b1bfcd6ccb533d5b9255711587910900` |
| Probe SHA-256 | `f901e98dd0df976768aa32b59b4c79ee00764a52253724aebe7fc3bdad8f8b85` | `8d18fdf87a6d51db84cefc4d5c6c432b01dbafe2b416e832cb74640c11980d6a` |

The product under test is a TCP byte tunnel, so the generator is a real TCP
client (`scripts/bench_v092_tcp_load`, standard library only), not an HTTP
load tool. Origin, proxy and generator run on one Docker bridge; **no host
port is published**. That removes Docker's user-space port forwarder from the
path.

### Scenarios

| ID | Load | Measured |
|---|---|---|
| S-TCP-01 | 128 closed-loop workers, new connection per request, 1 KiB request + echo, 1 s warm-up + **5 s** measured | connections/s, connect+round-trip p50/p95/p99, CPU, peak RSS |
| S-TCP-02 | 100 persistent connections, 4 KiB bidirectional echo, 3 s warm-up + **15 s** measured | messages/s, round-trip p50/p95/p99, CPU, peak RSS, derived memory per connection |

S-TCP-01 uses 128 workers because the first smoke with 32 workers was limited
by its own 17–30 ms closed-loop latency and, on netcup, exhausted the
generator's ephemeral ports after ~11k connects. The generator now runs in its
own network namespace with `ip_local_port_range=1024 65535` and
`tcp_tw_reuse=1`; the smoke was repeated with zero errors. The 5 s duration
bounds TIME_WAIT pressure. This is disclosed rather than hidden.

The **direct TCP echo origin** is calibrated with the same generator and the
same scenario settings. Five repetitions per cell alternate product order
(odd: EQProxy then HAProxy; even: the reverse). A series is valid only when
the direct sink is at least **1.20×** the faster proxy median and no measured
run is below **50 %** of the series median.

## 2. Results

All **40 measured runs** (5 reps × 2 scenarios × 2 products × 2 hosts) had
**zero generator errors** and completed HTTP-free byte echoes. Every series
cleared both validity gates. Raw files are in `netcup/raw/` and `oracle/raw/`;
`report.json` is the only source of the medians below.

### S-TCP-01 — new connections, 1 KiB echo

| Host | Product | Connections/s | p50 ms | p95 ms | p99 ms | CPU % one core | Peak MiB | Memory/conn |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| netcup | EQProxy | **864.0** | 134.96 | **284.96** | **334.02** | 186.2 | 31.74 | 5,443 B |
| netcup | HAProxy | 656.2 | **115.81** | 509.30 | 671.73 | 188.4 | 31.01 | **2,278 B** |
| Oracle | EQProxy | 1,541.2 | **16.76** | 321.08 | 397.34 | **171.2** | 30.77 | 3,188 B |
| Oracle | HAProxy | **1,650.8** | 18.75 | **255.83** | **275.36** | 174.5 | **27.24** | **825 B** |

- **netcup:** throughput/p50/CPU/peak RSS are **Comparable**, p95/p99 are
  **Better**, memory per connection is **Worse**. The p50 is high because 128
  new connections queue behind two saturated cores; this is a 2-CPU
  saturation result, not a low-latency claim.
- **Oracle:** throughput, p95, p99, peak RSS and memory/connection are
  **Worse**; p50 is **Comparable** and CPU is **Better**. EQProxy is about
  6.6 % behind on median connections/s and has the worse tail.

### S-TCP-02 — 100 persistent connections, 4 KiB bidirectional echo

| Host | Product | Messages/s | p50 ms | p95 ms | p99 ms | CPU % one core | Peak MiB | Memory/conn |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| netcup | EQProxy | **108,145** | 0.891 | 1.336 | **1.676** | **158.6** | **16.54** | 76,308 B |
| netcup | HAProxy | 93,639 | **0.528** | 1.407 | 37.391 | 162.1 | 25.83 | **22,528 B** |
| Oracle | EQProxy | **43,337** | **2.183** | **3.810** | **4.849** | **127.9** | **15.61** | 77,292 B |
| Oracle | HAProxy | 42,307 | 2.217 | 4.382 | 6.209 | 141.2 | 22.43 | **16,589 B** |

- **netcup:** throughput and p95 are **Comparable**; p50 is **Worse**, p99 is
  **Better**, CPU and peak RSS are **Better**, memory/connection is **Worse**.
  HAProxy's p99 median (37.4 ms, range 3.7–38.8) is a real spread in those five
  runs, not a hidden outlier removed from the raw data. The p99 advantage is
  therefore **not** generalised.
- **Oracle:** throughput and all three latency percentiles are **Comparable**;
  CPU and peak RSS are **Better**; memory/connection is **Worse**.
- **Memory per connection is a real limit in both scenarios and hosts.** It is
  derived as `(cgroup memory.peak − memory.current before the run) / established
  connections`, so it includes allocator and runtime growth, not just live
  buffers. The absolute S-TCP-02 values (76–77 KiB/connection) exceed the
  SPEC §7.3 **36 KiB/connection** target. That is a measured gap, not a
  benchmark artifact; fixing it is outside this measurement slice.

### Calibration and validity

| Host | Scenario | Direct sink | Faster proxy | Headroom | Stable | Valid |
|---|---|---:|---:|---:|---|---|
| netcup | S-TCP-01 | 32,435.0 conn/s | 864.0 | 37.541× | yes | yes |
| Oracle | S-TCP-01 | 15,192.8 conn/s | 1,650.8 | 9.203× | yes | yes |
| netcup | S-TCP-02 | 350,858.0 msg/s | 108,145.0 | 3.244× | yes | yes |
| Oracle | S-TCP-02 | 107,218.6 msg/s | 43,337.4 | 2.474× | yes | yes |

The direct sink is a **lower bound**: the slower of its generator and its Go
echo origin limits it. The headroom is large enough that the generator is not
the explanation for the proxy differences.

## 3. Verdicts

| Scenario | netcup | Oracle |
|---|---|---|
| S-TCP-01 | Mixed: Comparable throughput/p50/CPU/RSS; Better p95/p99; **Worse memory/connection** | **Worse** throughput/p95/p99/RSS/memory; Better CPU; Comparable p50 |
| S-TCP-02 | Comparable throughput/p95; **Worse p50**; Better p99/CPU/RSS; **Worse memory/connection** | Comparable throughput and all latencies; Better CPU/RSS; **Worse memory/connection** |

No TCP scenario is Better on every measured metric. Both scenarios are
**executed with valid evidence**, but they do not close the product goal of
Better-or-Comparable on every metric. The visible product gaps are:

1. **S-TCP-01 Oracle:** HAProxy is faster at new connections (1,650.8 vs
   1,541.2) and has the better p95/p99. EQProxy spends slightly less CPU.
2. **Memory per connection:** EQProxy uses 2.4–4.7× more peak growth per
   connection than HAProxy. At 100 streaming connections this is the
   difference between ~7.6 MiB and ~2.3 MiB (netcup), and ~7.7 MiB vs ~1.7 MiB
   (Oracle).
3. **Latency shape:** netcup's S-TCP-01 p50 is 135 ms for EQProxy vs 116 ms for
   HAProxy, while its tail is better; Oracle's tail is worse. The 128-worker
   closed loop saturates the 2-CPU quota, so p50 includes queueing.

No CPU profile or flamegraph was taken (`perf` is **NOT EXECUTED**:
`kernel.perf_event_paranoid=4` on netcup). The causes above are measured
differences, not attributed internal costs.

## 4. Integrity

- 20 raw files per host, 20 valid runs per host, **40 total**.
- Every run: `errors = 0`, `sanity = "ok"`, connections established, echo
  payload matched.
- `report.json` was produced by the harness, not edited. `checksums.sha256`
  covers the published raw files, configs, logs, report and this README; the
  binary hashes are in `report.json` and in the table above.
- Configs published verbatim: `configs/eqproxy.toml`, `configs/haproxy.cfg`.
- `run.log` and `run.out` are the harness output; `logs/` are the proxy
  container logs.
- The smoke runs (`smoke/`) were discarded: they exposed a mixed-scenario
  summariser bug and the ephemeral-port issue. No smoke number is used here.

## 5. Limitations and NOT EXECUTED

1. **No benchmark scenario remains.**
   Item 16 stays open at **18/18** for profiling/decisions.
2. **One host per architecture; components share it.** No CPU affinity and no
   separate generator/proxy/origin machines. Host load1 at start is published;
   it rises during the campaign.
3. **Closed-loop generator; no coordinated-omission correction.** Latencies
   are optimistic for both products. Histograms are in every raw file.
4. **S-TCP-01 uses 128 workers and 5 s.** That saturates the 2-CPU quota and
   deliberately stresses new-connection setup; it is not a general TCP
   throughput ceiling. The original 32-worker smoke was port-limited.
5. **Memory per connection is derived from cgroup peak growth**, not from a
   steady-state per-socket RSS probe. It includes allocator/runtime overhead.
6. **No profiler or flamegraph.** `perf` is NOT EXECUTED; no cause is claimed
   for the S-TCP-01 tail or the memory gap.
7. **TLS is not in this TCP path.** The TCP campaign uses the existing
   `eqproxy-bench:v092` image; the TLS session-resumption change is a separate
   source change and is not measured here.
8. **No commit, push or publication.**

## 6. Reproduce

```bash
# local-only harness: scripts/bench_v092_tcp.py and scripts/bench_v092_tcp_load
# cross-compile on the workstation, then on each evidence host:
python3 bench_v092_tcp.py --reps 5 --out run-<UTC> \
  --load-binary bin/tcp_load --probe-binary bin/probe \
  --eq-image eqproxy-bench:v092 --ha-image haproxy:3.4.6
```

The script builds the load image, creates the bridge, starts the origin,
calibrates, runs 20 measured cells, writes `report.json` and
`checksums.sha256`, and removes its containers and network.
