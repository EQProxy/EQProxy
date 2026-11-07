# v0.9.2 benchmark slice — S-TLS-01/02 on netcup and Oracle

Date: 2026-11-03 (UTC)
Status: **partial campaign; after this slice the roadmap is 18/18 scenarios
executed; no benchmark scenario remains. Item 16 stays open for profiling/decisions. Only S-TLS-01 is certified; the
original S-TLS-02 netcup claim is superseded by the correction below.**

> **Correction (2026-11-03):** the first S-TLS-02 netcup series was called
> "valid, near the gate" (1.203×). That was an artifact of the 306.3 RPS
> outlier lowering EQProxy's median. A 10-repetition repeat with the same
> Docker sink measured 5,269.2 RPS for the sink and 5,201.25 for EQProxy
> (**1.013×**) and reproduced the collapse (319.4 RPS at repetition 5). A
> corrected campaign with the sink on the host network measured **8,005.8 RPS**
> and 1.668× headroom, but repetition 5 still fell to **576.6 RPS**; the new
> stability gate therefore leaves the cell **Not comparable**. See §2.1. The
> 306.3 RPS run is retained in the original raw data; it is not deleted.

This directory contains the **S-TLS-01** and **S-TLS-02** runs from
`BENCHMARK_PLAN.md` on both supported architectures. The earlier S-H1-04/05
slice is in [`../v092-bench/`](../v092-bench/), the S-H1-01/02/03 slice in
[`../v092-bench-h1/`](../v092-bench-h1/) and the S-H2-01/02 slice in
[`../v092-bench-h2/`](../v092-bench-h2/).

The Mac was not used for measurements. It only cross-compiled the Linux Go TLS
calibration sink. Both campaigns ran on the owner's Linux evidence hosts, with
the native `eqproxy-bench:v092` image already used by the H1/H2 campaigns. Each
**proxy container** was limited to **2 GiB of memory and 2 CPUs**; the origin,
the TLS calibration sink and `oha` stayed outside those limits.

```text
benchv092-tls-eq: 2147483648 bytes  2000000000 NanoCpus
benchv092-tls-ha: 2147483648 bytes  2000000000 NanoCpus
```

## 1. Design and disclosure

| Field | netcup-bench | oracle-quasar |
|---|---|---|
| Architecture | `x86_64` / amd64 | `aarch64` / arm64 |
| Host | AMD EPYC 9645, 8 vCPU exposed, ~15 GiB, Ubuntu 24.04, Docker 29.6.1 | Oracle Ampere, 4 vCPU, ~23 GiB, Ubuntu 24.04, Docker 29.1.3 |
| EQProxy image | `eqproxy-bench:v092` `sha256:a461b0f678128017b3fd43bea47fb4f617cce50135b1379d7d3e45fe091d566c` | `eqproxy-bench:v092` `sha256:5a2e16f8b4d219efb753cf33f32094e29804d500fab00ade771220183f2a02a8` |
| HAProxy | `haproxy:3.4.6`, image ID `e273915691a8` | same image ID |
| Generator | `oha 1.16.0`, `/root/bin/oha-1.16.0` | `oha 1.16.0`, `/home/ubuntu/bin/oha` |
| TLS sink | Go 1.26.2 static `sink-amd64`, `sha256:2d9506fa302b9fac84d22cadd60172636fe66585cf105c9fe8923f5a5b8e5bb1` | Go 1.26.2 static `sink-arm64`, `sha256:de92a4e5d6b16805ea064494362cc60dd820619927efe5d40940c16021cfa8c6` |
| Certificate | fixture `cert.pem`, `sha256:0c6cab2ee350fe29fa65783b19baa55c19444de71e9926e35ef093be4d018017` | same |
| Repetitions | 5 measured per product/scenario + one discarded warm-up; S-TLS-02 repeat used 10 | same |
| Proxy limits | 2 GiB / 2 CPU per container | 2 GiB / 2 CPU per container |
| Host load1, S-TLS-01 start → end | 0.35 → 9.91 (the rise is the benchmark) | 0.15 → 7.97 (the rise is the benchmark) |
| Host load1, S-TLS-02 start → end | 0.76 → 6.72 (the rise is the benchmark) | 1.87 → 5.62 (the rise is the benchmark) |

- **Source tree:** working tree based on `64a1d77` with uncommitted changes,
  labeled `64a1d77+uncommitted-tree-20261103T0110Z` in both reports. The image
  IDs identify the exact code bytes; this is not a published release artifact.
- **Topology:** one Linux host; the plain HTTP origin, the TLS calibration sink,
  both proxies and `oha` share it. `oha` runs on the host against published
  loopback ports. One host per architecture; no separate generator/proxy/backend
  machines and no CPU affinity.
- **Origin:** `scripts/bench_v092_h1_backends.py`, one asyncio process, `/1k`,
  plain HTTP/1.1 keep-alive to the proxies.
- **TLS sink:** a real Go `net/http` TLS 1.3 / HTTP/1.1 server,
  `scripts/bench_v092_tls_sink/`, using the same fixture certificate, `/1k`,
  outside the proxy limits. It is used only to measure the generator's ceiling,
  not as a proxy result. The original runs published its port through Docker;
  the corrected S-TLS-02 run puts the sink on the host network, because that
  published-port path topped out at ~5.3k RPS and was itself a calibration
  ceiling.
- **Generator:** `oha 1.16.0`, closed-loop. Every TLS run used
  `--http-version 1.1 --cacert <fixture-cert>`; **no `--insecure`**.
  `--latency-correction` was not used because a closed-loop run has no honest
  offered rate for `-q`. Full raw histograms are published; percentiles are not
  coordinated-omission corrected.
- **TLS configuration:** both products terminate TLS 1.3 only, ALPN
  `http/1.1`, with the same certificate and `TLS_AES_256_GCM_SHA384`. The
  actual handshake was checked with `openssl s_client` before measuring:

```text
EQProxy: TLSv1.3  TLS_AES_256_GCM_SHA384  ALPN http/1.1  verify OK
HAProxy: TLSv1.3  TLS_AES_256_GCM_SHA384  ALPN http/1.1  verify OK
```

- **Measured configs:** `configs/s-tls-eqproxy.toml` and
  `configs/s-tls-haproxy.cfg` in each run directory. EQProxy sets
  `min_version = "1.3"`, `max_version = "1.3"`, `alpn = ["http/1.1"]`;
  HAProxy sets `ssl-default-bind-ciphersuites TLS_AES_256_GCM_SHA384`,
  `ssl-min-ver TLSv1.3`, `ssl-max-ver TLSv1.3` and
  `bind ... ssl crt ... alpn http/1.1`.
- **S-TLS-01:** 50 concurrent keep-alive connections × 10 s, `GET /1k`;
  one discarded 5 s warm-up per product.
- **S-TLS-02:** 20 concurrent connections × 5 s, a **new TLS handshake and
  connection per request** (`--disable-keepalive`), `GET /1k`; one discarded
  2 s warm-up per product. The shorter window avoids ephemeral-port/TIME_WAIT
  exhaustion; both products use identical client settings.
- **Order:** product order alternates every repetition; each report records it.
- **Corrected S-TLS-02 host load1:** 2.96–8.43 across the ten EQProxy runs
  (`disclosure.results...host_load1_before`); it rises with the benchmark, as in
  the original table.
- **Errors:** zero client errors and only HTTP 200 responses in all 40 original
  measured runs. The 40 repeat runs also had zero errors and only HTTP 200. The
  harness aborts if a run contains any error or non-200 status.

### Calibration and validity gate

The direct TLS sink is a lower bound on what the client can push. A series is
valid only when the sink exceeds **1.20×** the faster proxy median RPS and no
measured run falls below 50 % of the series median (the stability gate).

| Host | Scenario | Sink RPS | Generator CPU %1core | Faster proxy median | Headroom | Series valid |
|---|---|---:|---:|---:|---:|---|
| netcup | S-TLS-01 | 119,475.8 | 179.8 | 40,195.9 | 2.972 | **yes** |
| netcup | S-TLS-02, original 5 rep, Docker sink | 5,470.8 | 156.8 | 4,547.1 | 1.203 | **superseded** |
| netcup | S-TLS-02, 10 rep, Docker sink | 5,269.2 | 161.7 | 5,201.25 | 1.013 | **no — headroom** |
| netcup | S-TLS-02, 10 rep, host sink | 8,005.8 | 204.9 | 4,798.65 | 1.668 | **no — unstable** |
| Oracle | S-TLS-01 | 32,665.0 | 107.7 | 13,239.2 | 2.467 | **yes** |
| Oracle | S-TLS-02 | 2,237.9 | 76.5 | 1,917.7 | 1.167 | **no — Not comparable** |

S-TLS-02 on Oracle does not clear the predeclared gate. The generator used only
76.5 % of one core, so the limiting component is the TLS sink's handshake
ceiling, not client CPU saturation. Its raw values are published but **none of
its classifications is promoted**. A 40-connection pilot on Oracle was also
tried (headroom 1.031) and is not used as evidence.

S-TLS-02 on netcup is **not certified**. The original 1.203× was measured
against EQProxy's outlier-depressed median (4,547.1 RPS; one run at 306.3).
The 10-repetition repeat with the same Docker path raised the EQProxy median to
5,201.25 RPS and lowered the sink to 5,269.2 RPS: headroom **1.013×**, below
the 1.20 gate, and the collapse reproduced at 319.4 RPS in repetition 5. The
corrected host-network sink removes the Docker published-port ceiling
(8,005.8 RPS; headroom 1.668×), but repetition 5 then fell to 576.6 RPS. The
series is marked **Not comparable** by the stability gate; the raw direction
is not promoted. The host and Docker sink controls (7,871.6–8,629.2 and
5,709.7–5,934.3 RPS, no collapse) show the generator and sink can sustain the
shape; they do not identify the proxy-side cause.

## 2. Results

The S-TLS-01 and original S-TLS-02 tables are medians of five runs; the
corrected S-TLS-02 table is medians of ten. Ranges are min–max. CPU is the
container's cgroup CPU over wall time in percent of one core. Memory is
container RSS from `/proc`.

### netcup-bench — S-TLS-01

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 40,195.9 (30,563.5–42,164.0) | 33,937.0 (31,612.7–38,216.2) | 1.184 | Comparable |
| p50 ms | 1.226 (1.171–1.487) | 1.355 (1.204–1.432) | 1.105 | Comparable |
| p95 ms | 1.623 (1.539–2.722) | 2.585 (2.188–2.838) | 1.593 | Comparable |
| p99 ms | 1.889 (1.770–4.265) | 4.029 (3.944–4.251) | 2.133 | Comparable |
| CPU %1core | 163.8 (160.7–172.9) | 189.0 (182.4–190.2) | 0.867 | **Better** |
| RSS MiB | 24.81 | 61.02 | 0.407 | **Better** |
| Errors | 0 | 0 | — | — |

RPS and all latency ranges overlap, so they are Comparable. EQProxy uses less
CPU and memory; no metric is Worse. The product goal is met for this cell.

### Oracle-quasar — S-TLS-01

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 13,239.2 (13,203.5–13,387.4) | 10,997.0 (10,441.0–11,180.4) | 1.204 | **Better** |
| p50 ms | 3.643 (3.589–3.644) | 4.100 (3.958–4.360) | 1.125 | **Better** |
| p95 ms | 5.770 (5.681–5.782) | 8.436 (8.165–8.736) | 1.462 | **Better** |
| p99 ms | 6.929 (6.758–7.007) | 11.410 (11.026–11.798) | 1.647 | **Better** |
| CPU %1core | 111.4 (110.6–112.7) | 136.6 (127.6–140.9) | 0.816 | **Better** |
| RSS MiB | 21.57 | 55.39 | 0.389 | **Better** |
| Errors | 0 | 0 | — | — |

All measured metrics are Better on arm64. This is the strongest TLS cell.

### netcup-bench — S-TLS-02, original 5 rep (superseded raw)

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 4,547.1 (306.3–5,453.1) | 2,323.8 (2,292.0–2,537.3) | 1.957 | Not comparable |
| p50 ms | 3.920 (3.554–63.767) | 5.568 (5.037–5.731) | 1.420 | Not comparable |
| p95 ms | 6.894 (5.151–115.599) | 36.062 (35.481–37.076) | 5.231 | Not comparable |
| p99 ms | 10.289 (6.089–131.419) | 42.713 (42.357–45.030) | 4.151 | Not comparable |
| CPU %1core | 191.4 (186.5–197.5) | 199.4 (199.1–199.6) | 0.960 | Not comparable |
| RSS MiB | 24.29 | 62.20 | 0.391 | Not comparable |
| Errors | 0 | 0 | — | — |

These are the original raw medians, retained for traceability. The automated
range-overlap rule would have said Comparable for RPS/latencies and Better for
CPU/RSS, but the 306.3 RPS run (p95 115.6 ms) made that classification unsafe.
The series is not promoted; see the 10-repetition check below.

### netcup-bench — S-TLS-02, 10 rep, corrected host-network sink

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 4,798.65 (576.6–5,256.0) | 2,314.60 (2,040.9–2,473.5) | 2.073 | **Not comparable** |
| p50 ms | 3.886 (3.667–20.134) | 5.640 (5.158–6.509) | 1.453 | **Not comparable** |
| p95 ms | 6.006 (5.292–80.444) | 36.460 (36.077–37.836) | 6.071 | **Not comparable** |
| p99 ms | 7.222 (6.407–99.349) | 43.133 (41.881–43.791) | 5.973 | **Not comparable** |
| CPU %1core | 191.5 (190.2–197.8) | 199.15 (198.7–199.7) | 0.962 | **Not comparable** |
| RSS MiB | 23.91 | 61.96 | 0.386 | **Not comparable** |
| Errors | 0 | 0 | — | — |

The calibration now clears the headroom gate (8,005.8 RPS, **1.668×**), so the
sink is no longer the limiting component. The series still fails the stability
gate: repetition 5 served 576.6 RPS with p95 80.4 ms while the other nine were
4,582.9–5,256.0 RPS. All responses were 200 and the proxy CPU stayed at
197.8 % of one core during the collapse. The result is **Not comparable**, not
a win; a profiled investigation is pending.

### Oracle-quasar — S-TLS-02 (raw only; Not comparable)

| Metric | EQProxy | HAProxy | EQ/HA | Classification |
|---|---:|---:|---:|---|
| RPS | 1,917.7 (1,890.0–1,939.6) | 1,414.4 (1,351.4–1,418.1) | 1.356 | **Not comparable** |
| p50 ms | 9.959 (9.815–10.120) | 12.910 (12.777–13.314) | 1.296 | **Not comparable** |
| p95 ms | 15.522 (15.076–15.733) | 25.265 (23.837–26.859) | 1.628 | **Not comparable** |
| p99 ms | 19.030 (18.327–19.673) | 35.455 (33.947–39.363) | 1.863 | **Not comparable** |
| CPU %1core | 124.9 (123.5–126.5) | 187.8 (186.6–189.3) | 0.665 | **Not comparable** |
| RSS MiB | 22.03 | 53.98 | 0.408 | **Not comparable** |
| Errors | 0 | 0 | — | — |

The raw direction favors EQProxy on every metric, but the predeclared 1.20×
calibration gate fails (1.167×). The result is therefore **Not comparable**, not
a certified win or loss. It remains published so the next campaign can improve
the sink rather than hide the cell.

## 3. Integrity

Each host produced **11 raw `oha` JSON files per original scenario** (5
repetitions × 2 products + one calibration), so the original slice added **44
raw files**. The two netcup S-TLS-02 repeats add **21 each**, for **86 campaign
raw files** in total. The 10 sink-control files are separate. SHA-256 values in
each `report.json` were recomputed against the local copies:

```text
netcup S-TLS-01: 11 files, 0 mismatches
netcup S-TLS-02 (original): 11 files, 0 mismatches
oracle S-TLS-01: 11 files, 0 mismatches
oracle S-TLS-02: 11 files, 0 mismatches
netcup S-TLS-02 repeat: 21 files, 0 mismatches
netcup S-TLS-02 hostnet: 21 files, 0 mismatches
netcup sink controls: 10 files; checksums.sha256
```

The reports, raw files, configs, certificates and `run.log` are under:

```text
netcup/s-tls-01/    netcup/s-tls-02/
netcup/s-tls-02-repeat/    netcup/s-tls-02-hostnet/
netcup/controls/
oracle/s-tls-01/    oracle/s-tls-02/
```

The full response-time histograms are in the raw generator JSON. Calibration is
stored under `calibration` in each `report.json` and is not counted as a proxy
measurement. The TLS sink binary hashes are above; the source is
`scripts/bench_v092_tls_sink/main.go` (maintainer-local, like `scripts/`).

## 4. Reproduction

The TLS sink was cross-compiled on the Mac (not measured there) and copied to
the hosts:

```bash
cd scripts/bench_v092_tls_sink
for arch in amd64 arm64; do
  CGO_ENABLED=0 GOOS=linux GOARCH="$arch" GO111MODULE=off \
    go build -trimpath -ldflags=-s -o "/tmp/eqproxy-tls-sink/sink-$arch" .
done
```

Campaign command on netcup (Oracle uses its `sink-arm64` and
`$HOME/bin/oha`):

```bash
cd /root/eqproxy-v092
SCENARIOS=S-TLS-01 \
EQ_IMAGE=eqproxy-bench:v092 \
OHA_BIN=/root/bin/oha-1.16.0 \
TLS_SINK_BIN=/root/bench-v092-tls/sink-amd64 \
REPS=5 OHA_SECS=10 WARMUP_SECS=5 TLS_NEW_CONNS=20 TLS_NEW_SECS=5 \
MAX_LOADAVG=2.0 MEMORY_LIMIT=2g CPU_LIMIT=2 \
python3 -u scripts/bench_v092_h1.py \
  --out /root/bench-v092-tls/s-tls-01 \
  --commit "64a1d77+uncommitted-tree-20261103T0110Z"
```

The corrected S-TLS-02 check uses the same command in a fresh `--out` with
`SCENARIOS=S-TLS-02 REPS=10`. Since this correction the harness starts the TLS
sink with `--network host`, records `host_load1_before/after` on every run and
marks a series `Not comparable` when any run is below 50 % of the series
median. The older `s-tls-02` and `s-tls-02-repeat` reports predate the host
network and stability-gate changes; they are retained as history.

## 5. Limitations and NOT EXECUTED

1. **18/18 scenarios executed.** With this slice, S-H1-01/02/03/04/05,
   S-H2-01/02, S-TLS-01/02, S-TCP-01/02, S-H3-01/02, S-STR-01, S-FAIL-01, S-CFG-01,
   S-OVL-01 and S-PLG-01 were executed; only S-TLS-01 is certified. **No benchmark
   scenario remains.** Roadmap item 16 stays open for profiling/decisions.
2. **Oracle S-TLS-02 is Not comparable:** the direct sink reaches only 1.167×
   the faster proxy, below the 1.20× gate. A 40-connection pilot on the same
   host was worse (1.031×) and is not used. The raw values are published but
   not promoted.
3. **netcup S-TLS-02 is Not comparable and the original 1.203× claim is
   superseded.** The 10-repetition Docker-sink repeat gives 1.013× and 319.4
   RPS at repetition 5; the corrected host-network sink gives 1.668× headroom
   but 576.6 RPS at repetition 5. The stability gate rejects the cell. The
   original raw data and the 306.3 RPS run are retained, not deleted.
4. **The S-TLS-02 collapse is not attributed.** Host and Docker sink controls
   sustained 5.7–8.6k RPS for five runs without collapse, so the generator and
   sink are not a generic ceiling; they do not exercise EQProxy's TLS path. No
   CPU profile exists, so the proxy-side cause is **not established**.
5. **Closed-loop generator, no coordinated-omission correction.** Full raw
   histograms are published; P50/P95/P99 remain optimistic for both products.
6. **One host per architecture; components share the host.** No CPU affinity
   and no separate generator/proxy/backend machines.
7. **TLS scope:** TLS 1.3 only, one cipher, no session resumption **in these
   runs** (the product now issues stateless TLS 1.3 tickets; this campaign did
   not measure them), no 0-RTT, no OCSP stapling, no packet loss, no browser
   client and no mTLS.
8. **No profiler or flamegraph.** `perf` remains **NOT EXECUTED**
   (`kernel.perf_event_paranoid=4` on netcup; no substitute profile is
   presented). The 306.3/319.4/576.6 RPS collapses have no CPU profile.
9. **The EQProxy image is built from a working tree**, not a published release
   artifact. The image IDs above identify the exact bytes used.
10. **Cross-host absolute RPS is not comparable:** netcup and Oracle have
    different CPUs, core counts and memory. Only per-host EQProxy/HAProxy ratios
    are reported.
11. **Veyra, Rust Audit and CodeGraph were NOT EXECUTED** for this TLS
    benchmark increment: it changes benchmark evidence and maintainer-local
    harness files, not Rust source. No audit is claimed. TLS 1.3 session
    resumption was implemented separately in
    [`../v092-tls-resumption.md`](../v092-tls-resumption.md); it is not
    measured by these runs.
12. **No commit, push or publication** was made for this increment.

## 6. Files

```text
netcup/
  s-tls-01/  report.json, raw/ (11 JSON), configs/, certs/, run.log
  s-tls-02/  report.json, raw/ (11 JSON), configs/, certs/, run.log (superseded)
  s-tls-02-repeat/  report.json, raw/ (21 JSON), configs/, certs/, run.log
  s-tls-02-hostnet/ report.json, raw/ (21 JSON), configs/, certs/, run.log
  controls/  README.md, 10 oha JSON, checksums.sha256
oracle/
  s-tls-01/  report.json, raw/ (11 JSON), configs/, certs/, run.log
  s-tls-02/  report.json, raw/ (11 JSON), configs/, certs/, run.log
```
