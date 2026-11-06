# v0.9.2 benchmark slice — S-H1-01/02/03 on netcup and Oracle

Date: 2026-11-02/03 (UTC)
Status: **partial campaign; this slice covers 3 scenarios. After the later
S-H2-01/02, S-TLS-01/02, S-TCP-01/02, S-H3-01/02, S-STR-01, S-FAIL-01, S-CFG-01, S-OVL-01
and S-PLG-01 campaigns the roadmap is 18/18 with no benchmark scenario remaining; item 16 stays open for profiling/decisions.**

This directory contains the S-H1-01, S-H1-02 and S-H1-03 runs from
`BENCHMARK_PLAN.md` on both supported architectures. The earlier S-H1-04/05
slice is in [`../v092-bench/`](../v092-bench/), and the later S-H2-01/02
slice is in [`../v092-bench-h2/`](../v092-bench-h2/); the later TLS slice is in
[`../v092-bench-tls/`](../v092-bench-tls/).

The Mac was not used for measurements. Both campaigns ran on the owner's Linux
evidence hosts (`SERVIDORES.md`), with a native EQProxy image built on each
host. Each **proxy container** was limited to **2 GiB of memory and 2 CPUs**;
the origin and generator stayed outside those limits. The effective Docker
`HostConfig` recorded by the harness is:

```text
benchv092-eq: 2147483648 bytes  2000000000 NanoCpus
benchv092-ha: 2147483648 bytes  2000000000 NanoCpus
```

## 1. Design and disclosure

| Field | netcup-bench | oracle-quasar |
|---|---|---|
| Architecture | `x86_64` / amd64 | `aarch64` / arm64 |
| Host | AMD EPYC 9645, 8 vCPU exposed, ~15 GiB, Ubuntu 24.04, Docker 29.6.1 | Oracle Ampere, 4 vCPU, ~23 GiB, Ubuntu 24.04, Docker 29.1.3 |
| EQProxy image | `eqproxy-bench:v092` `sha256:a461b0f678128017b3fd43bea47fb4f617cce50135b1379d7d3e45fe091d566c` | `eqproxy-bench:v092` `sha256:5a2e16f8b4d219efb753cf33f32094e29804d500fab00ade771220183f2a02a8` |
| HAProxy | `haproxy:3.4.6`, image ID `e273915691a8` | same image ID |
| Generator | `oha 1.16.0` (`/root/bin/oha-1.16.0`) | `oha 1.16.0` (`/home/ubuntu/bin/oha`) |
| Repetitions | 5 measured per product/scenario + one discarded warm-up | same |
| Proxy limits | 2 GiB / 2 CPU per container | 2 GiB / 2 CPU per container |
| Host load1, start → end | 0.49 → 8.53 (the rise is the benchmark itself) | 0.25 → 8.92 (the rise is the benchmark itself) |

- **Source tree:** working tree based on `64a1d77` with uncommitted changes,
  labeled `64a1d77+uncommitted-tree-20261102T2348Z` in both reports. This is
  not a published release artifact.
- **Topology:** origin and both proxies in Docker containers on one bridge
  network; `oha` on the host against published loopback ports. One host per
  architecture; no separate generator/proxy/backend machines and no CPU
  affinity.
- **Origin:** `scripts/bench_v092_h1_backends.py` (maintainer-local, one
  asyncio process, `/1k` and `/1m`, `TCP_NODELAY`, honors `Connection: close`
  and HTTP/1.0).
- **Measured requests:** `GET /1k` for S-H1-01 and S-H1-03, `GET /1m` for
  S-H1-02.
- **Scenarios:** S-H1-01 = 50 connections × 10 s keep-alive; S-H1-02 = 50
  connections × 10 s keep-alive, 1 MiB; S-H1-03 = 10 connections × 5 s with
  `--disable-keepalive`. The S-H1-03 window is a declared deviation from the
  50 × 10 s used by the keep-alive scenarios; both products use identical
  client settings. It was chosen after a macOS pilot hit `EADDRNOTAVAIL`; the
  Linux hosts sustained the reduced churn with only HTTP 200 responses.
- **Generator correction:** `--latency-correction` is **not** used.
  `oha 1.14+` ignores it without `-q`, and a closed-loop run has no honest
  offered rate for `-q`. Full raw response-time histograms are published, but
  P50/P95/P99 are not coordinated-omission corrected.
- **Warm-up:** one discarded 5 s run for S-H1-01/02, 2 s for S-H1-03, per
  product and scenario.

### Calibration and validity gate

The harness first runs the same generator against the origin directly. It
marks a scenario **invalid / Not comparable** when that direct sink does not
exceed 1.20× the faster proxy's median RPS. This is deliberately conservative.

| Host | Scenario | Direct sink RPS | Generator CPU %1core | Headroom vs faster proxy | Series valid |
|---|---|---:|---:|---:|---|
| netcup | S-H1-01 | 45,074.4 | 85.7 | 1.145 | no |
| netcup | S-H1-02 | 4,808.1 | 80.4 | 1.536 | yes |
| netcup | S-H1-03 | 2,255.5 | 27.8 | 0.346 | no |
| Oracle | S-H1-01 | 20,164.3 | 69.9 | 1.286 | yes |
| Oracle | S-H1-02 | 2,311.2 | 70.2 | 1.761 | yes |
| Oracle | S-H1-03 | 3,315.4 | 42.6 | 0.843 | no |

The S-H1-03 gate needs a caveat: the direct Python origin accepts a new
backend connection per direct request, while both proxies reuse backend
connections. The direct sink can therefore be **slower** than a proxy even
when the generator is not saturated. The gate rejects S-H1-03 on both hosts;
that is a limitation of this calibration, not proof that the proxies agree.
The generator CPU (27.8 % on netcup, 42.6 % on Oracle) does not show generator
saturation.

## 2. Results

All values are medians of five runs; ranges are min–max. RSS is process RSS
reported by `/proc`, not image size. CPU is the sum of container process
`utime+stime` over wall time, in percent of one core.

### netcup-bench (amd64, 8 vCPU host, proxy 2 GiB / 2 CPU)

| Scenario | Product | RPS (min–max) | p95 ms (min–max) | CPU %1core | RSS MiB | Report verdict |
|---|---|---:|---:|---:|---:|---|
| S-H1-01 | EQProxy | 39,353.7 (32,513.3–39,931.9) | 1.709 (1.631–1.977) | 153.7 | 18.9 | Not comparable (cal. 1.145) |
| S-H1-01 | HAProxy | 33,358.7 (21,064.7–34,775.7) | 2.413 (2.272–4.551) | 159.1 | 52.4 | Not comparable (cal. 1.145) |
| S-H1-02 | EQProxy | 3,129.9 (2,890.0–3,622.8) | 24.488 (21.168–26.765) | 197.6 | 19.5 | Comparable RPS; Better p95/p99 |
| S-H1-02 | HAProxy | 2,749.9 (2,725.9–3,028.8) | 36.827 (32.485–38.742) | 198.8 | 52.5 | Comparable RPS; Better p95/p99 |
| S-H1-03 | EQProxy | 1,388.2 (415.9–5,282.0) | 44.690 (2.907–53.818) | 188.4 | 18.3 | Not comparable (cal. 0.346); raw worse |
| S-H1-03 | HAProxy | 6,510.4 (5,739.7–6,536.7) | 2.893 (2.866–3.291) | 113.9 | 52.4 | Not comparable (cal. 0.346); raw worse |

S-H1-02 is the only netcup scenario that passes the gate. Both proxies are
CPU-saturated at about 198 % (the 2-CPU cap); the RPS difference is inside the
observed spread, while EQProxy's p95/p99 and RSS are outside HAProxy's range.
The report's "Better CPU" row for S-H1-02 is not treated as an efficiency
claim because both products sit at the cap.

S-H1-03's raw netcup result is a large visible disadvantage for EQProxy
(median 1,388 vs 6,510 RPS; p95 44.7 vs 2.9 ms), but the predeclared
calibration gate rejects the series. It is **not promoted to Worse** and no
optimization was attempted. The spread is also large (415.9–5,282.0 RPS for
EQProxy) and the proxy reached 188 % of one core, so the next step is a valid
sink calibration plus profiling, not a code change based on this run.

### oracle-quasar (arm64, 4 vCPU host, proxy 2 GiB / 2 CPU)

| Scenario | Product | RPS (min–max) | p95 ms (min–max) | CPU %1core | RSS MiB | Report verdict |
|---|---|---:|---:|---:|---:|---|
| S-H1-01 | EQProxy | 15,676.2 (15,300.3–15,910.8) | 4.837 (4.823–5.055) | 106.9 | 16.8 | Better |
| S-H1-01 | HAProxy | 12,775.3 (12,704.6–12,866.6) | 6.834 (6.634–6.991) | 107.5 | 48.7 | Better |
| S-H1-02 | EQProxy | 1,312.6 (1,296.1–1,320.8) | 60.049 (59.482–61.363) | 131.6 | 18.1 | Better |
| S-H1-02 | HAProxy | 965.1 (942.8–983.1) | 92.617 (85.999–97.561) | 137.6 | 48.7 | Better |
| S-H1-03 | EQProxy | 3,934.5 (2,443.7–4,130.3) | 3.731 (3.543–21.033) | 97.1 | 16.5 | Not comparable (cal. 0.843); raw slightly better |
| S-H1-03 | HAProxy | 3,443.8 (3,345.2–3,506.8) | 4.755 (4.566–4.939) | 94.9 | 48.8 | Not comparable (cal. 0.843); raw slightly better |

S-H1-01 and S-H1-02 pass the gate on arm64; EQProxy is Better on RPS,
P50/P95/P99 and RSS, while CPU is Comparable on S-H1-01 and Better on S-H1-02.
S-H1-03 again fails the gate; its raw median is slightly better for EQProxy
but its range is wider (2,443.7–4,130.3 RPS), so the raw result is not promoted
to a certified Better either.

## 3. Integrity

Each host produced **33 raw `oha` JSON files** (3 scenarios × 2 products ×
5 repetitions + 3 calibration runs). The SHA-256 values in each
`report.json` were recomputed against the local copies:

```text
netcup: 33 files, 0 mismatches
oracle: 33 files, 0 mismatches
```

The reports, configs and `run.log` are in [`netcup/`](netcup/) and
[`oracle/`](oracle/). Raw files retain the full response-time histograms.
Both images were built natively on their host; neither was built on the Mac
or pushed to a registry.

## 4. Reproduction

The harness and origin are maintainer-local (`scripts/` is local-only and is
not published under `docs/`). The source tree was transferred with:

```bash
rsync -a --exclude target --exclude .git --exclude .codegraph \
  --exclude .veyra --exclude docs/evidence --exclude 'ROADMAP EQPROXY' \
  ./ netcup-bench:/root/eqproxy-v092/
rsync -a --exclude target --exclude .git --exclude .codegraph \
  --exclude .veyra --exclude docs/evidence --exclude 'ROADMAP EQPROXY' \
  ./ oracle-quasar:eqproxy-v092/
```

Native images:

```bash
ssh netcup-bench 'cd /root/eqproxy-v092 && \
  docker build --pull -f docker/Dockerfile -t eqproxy-bench:v092 .'
ssh oracle-quasar 'cd "$HOME/eqproxy-v092" && \
  docker build --pull -f docker/Dockerfile -t eqproxy-bench:v092 .'
```

Campaigns (both launched from the synced tree; `MEMORY_LIMIT=2g` and
`CPU_LIMIT=2` are applied to each proxy container):

```bash
ssh netcup-bench 'cd /root/eqproxy-v092 && \
  EQ_IMAGE=eqproxy-bench:v092 OHA_BIN=/root/bin/oha-1.16.0 \
  REPS=5 OHA_SECS=10 WARMUP_SECS=5 NEW_CONNS=10 NEW_SECS=5 \
  MAX_LOADAVG=2.0 MEMORY_LIMIT=2g CPU_LIMIT=2 \
  python3 -u scripts/bench_v092_h1.py --out /root/bench-v092-h1 \
  --commit "64a1d77+uncommitted-tree-20261102T2348Z"'

ssh oracle-quasar 'cd "$HOME/eqproxy-v092" && \
  EQ_IMAGE=eqproxy-bench:v092 OHA_BIN="$HOME/bin/oha" \
  REPS=5 OHA_SECS=10 WARMUP_SECS=5 NEW_CONNS=10 NEW_SECS=5 \
  MAX_LOADAVG=2.0 MEMORY_LIMIT=2g CPU_LIMIT=2 \
  python3 -u scripts/bench_v092_h1.py --out "$HOME/bench-v092-h1" \
  --commit "64a1d77+uncommitted-tree-20261102T2348Z"'
```

Evidence was copied back with `rsync -a <host>:<run-dir>/` into
`netcup/` and `oracle/` respectively.

## 5. Limitations and NOT EXECUTED

1. This slice adds 3 scenarios. Together with S-H1-04/05, S-H2-01/02,
   S-TLS-01/02 (only S-TLS-01 certified), S-TCP-01/02, S-H3-01/02, S-STR-01,
   S-FAIL-01, S-CFG-01, S-OVL-01 and S-PLG-01, the roadmap has **18/18 executed**; **no benchmark scenario remains**. Roadmap item 16 stays open for profiling/decisions.
2. S-H1-01 on netcup and S-H1-03 on both hosts fail the predeclared
   calibration gate. Their raw values are published but are **not** promoted
   to a certified comparison.
3. S-H1-03 on netcup is a visible raw disadvantage for EQProxy; no code
   optimization was made because the calibration is not valid. A valid sink
   calibration and a profile are still required.
4. The generator is closed-loop and not coordinated-omission corrected; the
   full histograms are published. A correcting generator with an honest
   offered rate remains pending.
5. One host per architecture; generator, origin and proxies share the host.
   No separate machines and no CPU affinity. The netcup host also had
   `envoy-p1`, `prof-origin` and a BuildKit container running, but
   `docker stats` sampled them at 0.38 %, 0.01 % and 0.00 % during the run.
6. `perf` counters were not executed for the proxy: the host reports
   `kernel.perf_event_paranoid=4`, and a `perf stat` probe collected no
   cycles. No substitute profile is presented as evidence.
7. The EQProxy image is built from a working tree, not a published release
   artifact. The image IDs above identify the exact code bytes used.
8. Cross-host absolute RPS is not comparable: the hosts have different CPUs,
   core counts and memory. Only the per-host EQProxy/HAProxy ratios are
   reported.
9. An initial arm64 launch was aborted before any measurement because the
   post-build load1 was 2.42 > `MAX_LOADAVG=2.0`. It was relaunched when load1
   was 0.25; the aborted attempt produced no raw data and is not counted.

## 6. Files

```text
netcup/
  configs/s-h1-eqproxy.toml
  configs/s-h1-haproxy.cfg
  raw/                       33 oha JSON files
  report.json                full disclosure, summaries and checksums
  run.log                    console log
oracle/
  configs/s-h1-eqproxy.toml
  configs/s-h1-haproxy.cfg
  raw/                       33 oha JSON files
  report.json                full disclosure, summaries and checksums
  run.log                    console log
```
