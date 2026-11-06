# v0.9.2 benchmark slice — S-H2-01/02 on netcup and Oracle

Date: 2026-11-02/03 (UTC)
Status: **partial campaign; roadmap item 16 remains open for profiling/decisions
(18/18 scenarios executed; none remains).**

**Post-campaign code change (2026-11-04).** The h2 client-stream pipe default
was reduced from 64 KiB to 16 KiB under ADR-0048. The measurements in
sections 2–4 were taken with the 64 KiB image and remain the reference for
that build. The five-repetition S-H2-01 re-measurement with the 16 KiB release
runtime was executed (`run-20261104T182135Z`, `run-20261104T182404Z`); the
result is in section 5 and is **not a throughput win**.

This directory contains the **S-H2-01** and **S-H2-02** runs from
`BENCHMARK_PLAN.md` on both supported architectures. The earlier
S-H1-04/05 slice is in [`../v092-bench/`](../v092-bench/), and the
S-H1-01/02/03 slice is in [`../v092-bench-h1/`](../v092-bench-h1/), and the
later TLS slice is in [`../v092-bench-tls/`](../v092-bench-tls/).

The Mac was not used for measurements. Both campaigns ran on the owner's Linux
evidence hosts (`SERVIDORES.md`), with the native `eqproxy-bench:v092` image
already used by the H1 campaign. The Go generator was **cross-compiled on the
Mac** for Linux, but the binary was executed only on netcup and Oracle. Each
**proxy container** was limited to **2 GiB of memory and 2 CPUs**; the origin,
the h2 calibration sink and the generator stayed outside those limits.

```text
h2bench-eqproxy: 2147483648 bytes  2000000000 NanoCpus
h2bench-haproxy: 2147483648 bytes  2000000000 NanoCpus
```

## 1. Design and disclosure

| Field | netcup-bench | oracle-quasar |
|---|---|---|
| Architecture | `x86_64` / amd64 | `aarch64` / arm64 |
| Host | AMD EPYC 9645, 8 vCPU exposed, ~15 GiB, Ubuntu 24.04, Docker 29.6.1 | Oracle Ampere, 4 vCPU, ~23 GiB, Ubuntu 24.04, Docker 29.1.3 |
| EQProxy image | `eqproxy-bench:v092` `sha256:a461b0f678128017b3fd43bea47fb4f617cce50135b1379d7d3e45fe091d566c` | `eqproxy-bench:v092` `sha256:5a2e16f8b4d219efb753cf33f32094e29804d500fab00ade771220183f2a02a8` |
| HAProxy | `haproxy:3.4.6`, image ID `e273915691a8` | same image ID |
| Generator | Go 1.26.2, `golang.org/x/net/http2 v0.42.0`, `load-amd64` `sha256:a06f55c8ef1aa34e83bd7014a1b552e3cf4dfe464faf42bd717c5fac60cd4381` | same source, `load-arm64` `sha256:2e82cae45de79c800cf73c7edf8286ba8af10357b53ff625a46157fd8e18df68` |
| Probe | `probe-amd64` `sha256:872ade6362841e2b6440de30896222aed1d06f606db014989dbf988dc3901357` | `probe-arm64` `sha256:b6eb02ee786fc2b37e5f0cf51efd620f3fa58c23d11fe0a352da2e2b37645764` |
| Certificate | `eqproxy-net/tests/fixtures/cert.pem`, CN=localhost, SAN DNS:localhost + IP:127.0.0.1 | same |
| Repetitions | 5 measured per product/scenario | same |
| Proxy limits | 2 GiB / 2 CPU per container | 2 GiB / 2 CPU per container |
| Host load1, S-H2-01 start → end | 0.00 → 11.73 (rise is the benchmark) | 0.14 → 8.99 (rise is the benchmark) |
| Host load1, S-H2-02 start → end | 1.13 → 6.29 (rise is the benchmark) | 0.46 → 7.38 (rise is the benchmark) |

- **Source tree:** working tree based on `64a1d77` with uncommitted changes.
  The image IDs above identify the exact code bytes. This is not a published
  release artifact.
- **Topology:** origin, h2 sink, both proxies and the generator are containers
  on one bridge network. Nothing traverses a published host port. One host per
  architecture; no separate generator/proxy/backend machines and no CPU
  affinity.
- **Origin:** the Go harness's `origin` mode, one fixed body per
  path (`/1024`, `/1048576`), `Content-Length`, HTTP/1.1 keep-alive, no TLS,
  no logging in the hot path.
- **Sink:** the same Go binary in `sink` mode, a real TLS/ALPN h2 server with
  the fixture certificate, used only for generator calibration.
- **Generator:** one `http2.Transport` per connection with
  `StrictMaxConcurrentStreams`, so the requested connection and stream counts
  are what the server sees. It verifies the fixture CA and validates
  `HTTP/2.0`, status 200 and the exact body length. Closed-loop; **no
  coordinated-omission correction**.
- **S-H2-01:** 4 TLS connections × 100 concurrent streams (400 in flight),
  `GET /1024`, 3 s warm-up + **15 s** measured.
- **S-H2-02:** 4 connections × 8 streams (32 in flight), `GET /1048576`
  (1 MiB), 3 s + 15 s.
- **Campaigns:** S-H2-01 and S-H2-02 were run as **separate campaigns** on each
  host. The H1 harness showed that a combined run can leave conntrack and port
  state behind; no result from a combined ordering is used.
- **Calibration:** the same generator runs against the h2 sink before the proxy
  series. A series is valid when the sink exceeds **1.20×** the faster proxy's
  median RPS. All four series pass this gate.

### Calibration and validity gate

| Host | Scenario | Sink RPS | Generator CPU %1core | Faster proxy median | Headroom | Series valid |
|---|---|---:|---:|---:|---:|---|
| netcup | S-H2-01 | 121,858.5 | 316 | 66,958.0 | 1.820 | yes |
| netcup | S-H2-02 | 2,089.0 | 351 | 1,673.7 | 1.248 | yes |
| Oracle | S-H2-01 | 41,688.7 | 147 | 26,191.7 | 1.592 | yes |
| Oracle | S-H2-02 | 856.3 | 184 | 673.2 | 1.272 | yes |

S-H2-02's 1 MiB calibration is close to the measured cells (1.25–1.27×). The
gate passes, but small differences in that scenario are not presented as a
precise product advantage. The generator used 2.8–3.5 cores on netcup and
1.5–1.8 cores on Oracle for the 1 MiB cells, so the generator/origin pair is
part of the ceiling there.

## 2. Results

All values are medians of five runs; ranges are min–max. CPU is cgroup
`usage_usec` over wall time, in percent of one core. Memory is cgroup
`memory.peak`, not image size. All measured runs returned HTTP/2.0, status 200
and the exact body length; there were **zero client errors** in all 40 runs.

### netcup-bench — S-H2-01

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 40,476.7 (36,816.4–46,017.0) | 66,958.0 (62,307.5–74,756.0) | 0.605 | **Worse** |
| p50 ms | 9.768 (8.574–10.671) | 5.134 (4.295–5.255) | 1.903 | **Worse** |
| p95 ms | 15.264 (13.322–16.837) | 14.678 (13.709–17.549) | 1.040 | Comparable |
| p99 ms | 18.901 (16.031–19.902) | 24.125 (21.137–25.476) | 0.783 | **Better** |
| CPU %1core | 193.2 (193.1–193.3) | 194.5 (191.9–194.9) | 0.993 | Comparable |
| Memory MiB | 50.37 (48.84–60.91) | 39.48 (39.01–39.98) | 1.276 | **Worse** |
| Errors | 0 | 0 | — | — |

Both proxies are pinned at the 2-CPU cap. EQProxy is clearly behind on RPS and
p50, while its p99 is better and p95/CPU are inside the observed spread. The
overall scenario is **Worse**.

### Oracle-quasar — S-H2-01

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 18,560.0 (18,162.8–18,830.4) | 26,191.7 (25,083.6–27,430.8) | 0.709 | **Worse** |
| p50 ms | 21.397 (20.992–21.635) | 13.595 (13.149–14.363) | 1.574 | **Worse** |
| p95 ms | 32.206 (31.966–33.777) | 31.488 (29.200–32.129) | 1.023 | Comparable |
| p99 ms | 37.578 (36.748–40.752) | 42.562 (38.555–43.207) | 0.883 | Comparable |
| CPU %1core | 161.3 (159.2–162.0) | 159.4 (158.3–165.0) | 1.012 | Comparable |
| Memory MiB | 52.46 (49.57–66.53) | 35.27 (35.05–35.81) | 1.487 | **Worse** |
| Errors | 0 | 0 | — | — |

The direction is the same on arm64: RPS and p50 are worse, memory is worse,
CPU and p95 are comparable and p99 overlaps. Overall **Worse**.

### netcup-bench — S-H2-02

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| Throughput req/s | 1,602.5 (1,516.6–1,677.2) | 1,673.7 (1,639.1–1,775.4) | 0.957 | Comparable |
| Throughput MiB/s | 1,602.5 | 1,673.7 | 0.957 | Comparable |
| p50 ms | 19.776 (19.033–20.942) | 16.086 (14.040–16.233) | 1.229 | **Worse** |
| p95 ms | 28.780 (26.860–30.449) | 42.118 (41.603–44.258) | 0.683 | **Better** |
| p99 ms | 32.623 (30.695–35.189) | 47.014 (46.217–49.442) | 0.694 | **Better** |
| CPU %1core | 195.5 (195.3–195.6) | 196.2 (196.0–196.5) | 0.996 | Equal at the 2-CPU cap; no efficiency claim |
| Memory MiB | 98.27 (86.43–107.08) | 57.82 (53.61–67.41) | 1.700 | **Worse** |
| Errors | 0 | 0 | — | — |

RPS ranges overlap, so throughput is **Comparable**. EQProxy has a worse p50
but better p95/p99. Both proxies sit at the 2-CPU cap; the small CPU difference
is not an efficiency claim. Memory is 1.70× HAProxy's. Under the plan's rule
that every metric must be Better or Comparable, the scenario is **Worse**.

### Oracle-quasar — S-H2-02

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| Throughput req/s | 663.7 (654.2–667.5) | 673.2 (662.3–695.0) | 0.986 | Comparable |
| Throughput MiB/s | 663.7 | 673.2 | 0.986 | Comparable |
| p50 ms | 48.438 (47.980–48.907) | 44.566 (43.073–45.505) | 1.087 | **Worse** |
| p95 ms | 68.869 (67.418–69.377) | 79.577 (74.879–80.500) | 0.865 | **Better** |
| p99 ms | 77.408 (74.483–78.086) | 101.724 (100.077–105.605) | 0.761 | **Better** |
| CPU %1core | 176.4 (176.0–177.0) | 192.5 (187.7–194.2) | 0.916 | **Better** |
| Memory MiB | 83.81 (80.92–99.82) | 61.09 (59.82–62.83) | 1.372 | **Worse** |
| Errors | 0 | 0 | — | — |

Throughput is Comparable; EQProxy spends less CPU and has better p95/p99, but
its p50 and memory are worse. Overall **Worse**, not a win.

### Reading the results

- **No H2 scenario meets the product goal** of Better/Comparable on every
  measured metric. S-H2-01 is Worse on both architectures; S-H2-02 is Worse
  because p50 and memory are worse even where its tail and CPU are better.
- The **S-H2-01 p99 inversion** (EQProxy lower p99 than HAProxy on netcup,
  overlapping on Oracle) does not offset the large RPS/p50 gap. It is reported
  as measured, not used to relabel the scenario.
- The **backend-pool churn found in v0.7** did not reappear: all runs had zero
  errors/502s and the origin saw only 0.43–2.74 new backend connections per
  1,000 requests. This is a different image and a 2-CPU cap, so it is not a
  comparison with the v0.7 numbers.
- Memory is consistently higher in EQProxy (1.28–1.70×). No memory profile was
  taken in this slice; the per-stream h2→h1 translation remains a hypothesis,
  not a conclusion.

## 3. Integrity

Each host produced **10 raw JSON files per scenario** (5 repetitions × 2
products), plus the calibration result inside `report.json`; S-H2-01 and
S-H2-02 therefore add **40 raw files** total. The SHA-256 values in every
`report.json` were recomputed against the local copies:

```text
netcup S-H2-01: 10 files, 0 mismatches
netcup S-H2-02: 10 files, 0 mismatches
oracle S-H2-01: 10 files, 0 mismatches
oracle S-H2-02: 10 files, 0 mismatches
```

The reports, raw files, configs, certificates, container logs and `run.log` are
under:

```text
netcup/s-h2-01/    netcup/s-h2-02/
oracle/s-h2-01/    oracle/s-h2-02/
```

The full response-time histograms are in the raw generator JSON. The
calibration is stored in `report.json` under `calibration`; the raw calibration
run is not a proxy measurement and is not counted in the 40.

## 4. Reproduction

The harness is maintainer-local (`scripts/` is local-only and is not published
under `docs/`). The H2 generator and probe are the v07 Go sources in
`scripts/evidence/v07-h2-bench/`, cross-compiled for Linux:

```bash
cd scripts/evidence/v07-h2-bench
for arch in amd64 arm64; do
  CGO_ENABLED=0 GOOS=linux GOARCH="$arch" go build -trimpath -ldflags=-s \
    -o "/tmp/eqproxy-h2-bin/load-$arch" .
  CGO_ENABLED=0 GOOS=linux GOARCH="$arch" go build -trimpath -ldflags=-s \
    -o "/tmp/eqproxy-h2-bin/probe-$arch" ./probe
done
```

The campaign command on each host is:

```bash
MAX_LOADAVG=2.0 MEMORY_LIMIT=2g CPU_LIMIT=2 python3 -u scripts/bench_v092_h2.py \
  --reps 5 --scenarios S-H2-01 \
  --eq-image eqproxy-bench:v092 \
  --load-binary /root/bench-v092-h2/bin/load-amd64 \
  --probe-binary /root/bench-v092-h2/bin/probe-amd64
# repeat with S-H2-02 in a fresh campaign
```

Oracle uses `$HOME/bench-v092-h2/bin/load-arm64` and `probe-arm64`. The
campaign writes `docs/evidence/v092-bench-h2/run-<UTC>/`; its `report.json`
records the image IDs, generator hashes, limits, host load, calibration and
raw checksums.

## 5. S-H2-01 re-measurement with the 16 KiB default (2026-11-04)

The release runtime images were built natively (`--target runtime`) and run
with the same scenario, generator and five repetitions per host. The proxy
limit was 2 GiB / 2 CPU, as in the original campaign.

| Metric | netcup EQProxy | netcup HAProxy | EQ/HA | Oracle EQProxy | Oracle HAProxy | EQ/HA |
|---|---:|---:|---:|---:|---:|---:|
| RPS | 34 376.5 | 57 904.5 | 0.594 | 18 707.1 | 25 130.2 | 0.744 |
| p50 ms | 11.410 | 5.775 | 1.976 | 21.258 | 14.388 | 1.477 |
| p99 ms | 21.094 | 25.068 | 0.842 | 37.352 | 41.580 | 0.898 |
| CPU %1core | 192.8 | 193.5 | 0.996 | 161.1 | 162.7 | 0.990 |
| Peak MiB | 49.52 | 39.39 | 1.257 | 48.98 | 35.46 | 1.381 |
| Errors | 0 | 0 | — | 0 | 0 | — |

Both cells remain **Worse** on RPS and p50. Peak memory fell from 50.37 to
49.52 MiB (netcup) and from 52.46 to 48.98 MiB (Oracle) versus the original
64 KiB campaign, so the 16 KiB default is kept as a memory-bounded change.
The p99 inversion is reported as measured and does not relabel the scenario.

Integrity: 10 raw JSON files per host (5 repetitions × 2 products), all
SHA-256 checksums verified with **0 mismatches**. The images are
`eqproxy-bench:v093` (`0411c0ed717aa1af`, netcup) and `eqproxy-bench:v093`
(`0e896f704618811a`, Oracle); both report `eqproxy 0.9.1`. The source tree was the clean `84cba4b`
plus only the profiler/Dockerfile fixes. Evidence:

```text
docs/evidence/v092-bench-h2/run-20261104T182135Z/  # netcup amd64
docs/evidence/v092-bench-h2/run-20261104T182404Z/  # Oracle arm64
```

## 6. Limitations and NOT EXECUTED

1. **18/18 scenarios executed.** With the H1/TLS/TCP/H3/S-STR/S-FAIL/S-CFG/S-OVL/S-PLG slices,
   S-H1-01/02/03/04/05, S-H2-01/02, S-TLS-01/02, S-TCP-01/02, S-H3-01/02,
   S-STR-01, S-FAIL-01, S-CFG-01, S-OVL-01 and S-PLG-01 were executed; only S-TLS-01 is
   certified. **No benchmark scenario remains.** Roadmap item 16 stays open for profiling/decisions.
2. **One host per architecture; components share the host.** Generator,
   origin, sink and proxies contend for CPU; no CPU affinity and no separate
   machines. The rise in host load1 during each campaign is the benchmark
   itself.
3. **Closed-loop generator, no coordinated-omission correction.** P50/P95/P99
   are optimistic for both products; full histograms are published.
4. **H2-02 is close to the calibration ceiling** (1.248× netcup, 1.272×
   Oracle). Small throughput differences are not treated as decisive.
5. **Netcup's first H2-02 launch was refused before measuring** because the
   start load1 was 2.21 > `MAX_LOADAVG=2.0`; it was relaunched at 1.52. The
   refused attempt produced no raw data and is not counted.
6. **No profiler or flamegraph was taken in this campaign.** The memory and
   translation-cost observations are measured effects, not a CPU profile. A
   separate 2026-11-04 `profile` run is reported in
   `docs/evidence/v092-profiler-inferno.md`; it does not retroactively cover
   these campaign cells. `perf` remains **NOT EXECUTED**
   (`kernel.perf_event_paranoid=4` on netcup; no substitute profile is
   presented).
7. **One certificate/fixture** (TLS 1.3, ALPN h2), no session resumption in
   this campaign (the product now issues stateless tickets; not measured
   here), no 0-RTT, no h2c, no packet loss, no browser client, no H2 priority
   test.
8. **The original section 2–4 images were built from a working tree**, not a
   published release artifact; the image IDs identify the bytes used. The
   section 5 re-measurement used release `runtime` images built from the clean
   `84cba4b` tree plus only the profiler fixes, but those images are still not
   published.
9. **Cross-host absolute RPS is not comparable:** netcup and Oracle have
   different CPUs and core counts. Only per-host ratios are reported.

## 7. Files

```text
netcup/
  s-h2-01/  report.json, raw/ (10 JSON), configs/, certs/, logs/, run.log
  s-h2-02/  report.json, raw/ (10 JSON), configs/, certs/, logs/, run.log
oracle/
  s-h2-01/  report.json, raw/ (10 JSON), configs/, certs/, logs/, run.log
  s-h2-02/  report.json, raw/ (10 JSON), configs/, certs/, logs/, run.log
```

No commit, push or publication was made for this increment. Veyra, Rust Audit,
CodeGraph and Hindsight for the audit stack were **NOT EXECUTED** here because
this increment changed only benchmark evidence and maintainer-local harness
files; no Rust source was changed. The 2 GiB/2 CPU limits apply to each proxy
container, not to the generator, origin or sink.

The later 2026-11-04 profiler/release increment did change Rust source
(`eqproxy-cli/src/profile.rs`) and ran Veyra, Rust Audit and CodeGraph; those
results are reported in `docs/evidence/v092-profiler-inferno.md` and the
delivery, not retroactively in this benchmark note.
