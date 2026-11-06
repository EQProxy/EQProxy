# v0.9.2 benchmark slice — S-H1-01/02/03 vs static Envoy and HAProxy

Date: 2026-11-05 (UTC)

Status: **benchmark executed; the whole casilla 14 is now tracked in v0.9.4
because the operator-task study is NOT EXECUTED.**

This directory contains the three-product benchmark half of casilla 14:
EQProxy, **static Envoy v1.34.2** and **HAProxy 3.4.6**, on the three
HTTP/1.1 scenarios S-H1-01, S-H1-02 and S-H1-03 of
[`BENCHMARK_PLAN.md`](../../../BENCHMARK_PLAN.md) §3. The complete casilla,
including the operator study, was moved to
[`Roadmap 0.9.4`](../../../ROADMAP%20EQPROXY/ROADMAP/Roadmap%200.9.4.md) on
2026-11-05. This directory is a benchmark, not an operator study, and it does
not certify the product goal on its own. Envoy is a static bootstrap here: no
Istio, no xDS control plane and no service mesh.

The full 18-scenario v0.9.2 campaign is documented in the other
`docs/evidence/v092-bench-*` directories. This slice adds the direct
three-product comparison for S-H1-01/02/03; it does not repeat or replace the
18/18 result.

## 1. Scope and disclosure

| Field | Value |
|---|---|
| Run (UTC) | `2026-11-05T22:04:05Z`; final `report.json` written after the 5 repetitions |
| Source tree | `9913a69` plus the uncommitted working tree; report label `9913a69+bench-v092-envoy-20261105` |
| EQProxy image | `eqproxy-bench:v093`, ID `sha256:0411c0ed717aa1af...`; built from the working tree, not a published release artifact |
| HAProxy | `haproxy:3.4.6`, ID `e273915691a8`; `HAProxy version 3.4.6-56332c5 2026/09/28` |
| Envoy | `envoyproxy/envoy:v1.34.2`, ID `daca6a3f353b`; `1.34.2/Clean/RELEASE/BoringSSL` |
| Origin | `python:3.14-slim`, ID `a2b82f3c4855`; `scripts/bench_v092_h1_backends.py`, one asyncio process serving `/1k` and `/1m` |
| Host | `netcup-bench`: AMD EPYC 9645, 8 logical CPUs exposed, 15.6 GiB RAM, Linux 6.8.0-147-generic x86_64, Docker 29.6.1 |
| Generator | `oha 1.16.0`, `/root/bin/oha-1.16.0` |
| Proxy limits | 2 GiB memory and 2 CPUs per proxy container; applied `HostConfig` `2147483648 2000000000` for all three |
| Topology | Origin and the three proxies in Docker containers on one bridge network; `oha` on the host against published loopback ports |
| Host load1 | `0.17` at start; `7.75` at end (the rise is the benchmark itself) |
| Repetitions | 5 measured per product/scenario plus one discarded warm-up |

The host already had the long-running `envoy-p1` (`v1.31.0`), `prof-origin` and
a BuildKit container. They were not part of this campaign; no campaign-level
CPU or memory limit was applied to them, and no separate-machine isolation or
CPU affinity was used.

### Scenarios and generator

| ID | Payload | Connections | Measured window | Keep-alive |
|---|---:|---:|---:|---|
| S-H1-01 | 1 KiB | 50 | 10 s | yes |
| S-H1-02 | 1 MiB | 50 | 10 s | yes |
| S-H1-03 | 1 KiB | 10 | 5 s | no (`--disable-keepalive`) |

Warm-up was one discarded 5 s run for S-H1-01/02 and 2 s for S-H1-03, per
product and scenario. The run order was rotated per repetition:
`eqproxy → haproxy → envoy`, `haproxy → envoy → eqproxy`,
`envoy → eqproxy → haproxy`, then the first two orders again. The generator was
closed-loop; `--latency-correction` was not used. Full response-time histograms
are in `raw/`.

The three configurations are the complete files in `configs/`:

- EQProxy: HTTP listener, `maxconn = 4096`, one origin server.
- HAProxy: HTTP mode, `nbthread 2`, `maxconn 4096`, keep-alive enabled.
- Envoy: static bootstrap, HTTP connection manager, `--concurrency 2`, a
  single static cluster and the router filter.

## 2. Calibration and validity gate

Before the proxy series, the same generator measured the origin directly. The
predeclared gate in the harness rejects a series when the direct sink is below
**1.20×** the fastest proxy's median RPS, or when any measured run is below 50 %
of the series median. The gate was not changed after measuring.

| Scenario | Direct RPS | Generator CPU %1core | Headroom vs fastest proxy | Valid | Reason |
|---|---:|---:|---:|---|---|
| S-H1-01 | 51,917.5 | 82.4 | 1.016 | **no** | direct sink < 1.20× fastest proxy |
| S-H1-02 | 5,152.4 | 73.3 | 1.270 | **yes** | — |
| S-H1-03 | 2,127.0 | 24.2 | 0.179 | **no** | direct sink < 1.20× fastest proxy; a measured run < 50 % of the series median |

S-H1-01 is rejected because the direct origin reached 51.9 k RPS while HAProxy
reached 51.1 k: the generator/origin side is not enough above the proxy to
certify the difference as a proxy result. S-H1-03 is rejected for a second,
scenario-specific reason: the direct Python origin opens a **new upstream
connection per request**, while the proxies reuse upstream connections, so the
direct sink is not an upper bound for that scenario. Its raw numbers are kept,
but they are not promoted to a comparison or to a statement that one product is
faster.

## 3. Results

All values are medians of five runs; parentheses are min–max. CPU is the sum of
container process `utime+stime` over wall time, in percent of one core. RSS is
process RSS from `/proc`, not image size.

| Scenario | Product | RPS (min–max) | p50 ms (min–max) | p95 ms (min–max) | p99 ms (min–max) | CPU %1core | RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|
| S-H1-01 **Not comparable** | EQProxy | 45,164.4 (40,907.3–47,198.7) | 1.090 (0.995–1.171) | 1.413 (1.376–1.582) | 1.738 (1.512–1.891) | 151.9 | 18.6 |
| S-H1-01 **Not comparable** | HAProxy | 51,085.2 (46,517.4–52,484.2) | 0.952 (0.906–1.081) | 1.306 (1.293–1.437) | 1.508 (1.437–1.761) | 131.2 | 48.4 |
| S-H1-01 **Not comparable** | Envoy | 36,962.6 (35,013.0–42,155.7) | 1.311 (1.164–1.374) | 1.860 (1.625–2.231) | 2.299 (1.877–2.830) | 188.2 | 98.9 |
| S-H1-02 **Valid** | EQProxy | 3,525.1 (3,001.0–3,909.2) | 13.630 (12.371–15.814) | 21.495 (19.403–27.460) | 25.936 (23.335–33.682) | 198.0 | 19.3 |
| S-H1-02 **Valid** | HAProxy | 2,750.8 (2,172.6–2,885.6) | 17.636 (16.981–22.682) | 24.486 (23.153–29.658) | 28.950 (26.742–33.932) | 191.6 | 48.5 |
| S-H1-02 **Valid** | Envoy | 4,057.6 (3,826.4–4,258.0) | 12.130 (11.568–12.948) | 17.618 (16.486–18.684) | 20.096 (18.305–21.367) | 192.0 | 98.9 |
| S-H1-03 **Not comparable** | EQProxy | 4,518.0 (4,376.8–4,572.3) | 1.022 (0.892–1.051) | 9.839 (2.679–10.662) | 35.154 (22.755–41.065) | 168.2 | 17.9 |
| S-H1-03 **Not comparable** | HAProxy | 9,345.1 (8,502.8–9,428.0) | 0.984 (0.970–1.074) | 1.811 (1.779–2.043) | 2.598 (2.490–2.826) | 100.1 | 48.5 |
| S-H1-03 **Not comparable** | Envoy | 11,883.4 (11,547.8–14,086.5) | 0.804 (0.674–0.823) | 1.349 (1.170–1.399) | 1.680 (1.442–1.754) | 152.1 | 98.9 |

Only **S-H1-02** is an admissible comparison. Its report classification is:

| Pair | RPS | p50 | p95 | p99 | p99.9 | CPU | RSS |
|---|---|---|---|---|---|---|---|
| EQProxy vs HAProxy | **Better** | **Better** | Comparable | Comparable | Comparable | **Worse** | **Better** |
| EQProxy vs Envoy | Comparable | Comparable | **Worse** | **Worse** | **Worse** | **Worse** | **Better** |
| Envoy vs HAProxy | **Better** | **Better** | **Better** | **Better** | **Better** | Comparable | **Worse** |

The median ratios in `report.json` are:

| Pair | RPS ratio | p99 ratio |
|---|---:|---:|
| EQProxy / HAProxy | 1.281 | 0.896 |
| EQProxy / Envoy | 0.869 | 1.291 |
| Envoy / HAProxy | 1.475 | 0.694 |

S-H1-02 does **not** meet the “Better or Comparable on all metrics” goal:
EQProxy is Better than HAProxy on RPS, p50 and RSS, but Worse on CPU; against
Envoy, RPS and p50 are Comparable, while p95, p99, p99.9 and CPU are Worse. The
EQProxy RPS range overlaps Envoy's, so even the RPS verdict is Comparable, not
a win. Envoy beats HAProxy on the measured RPS and latency rows, but uses more
memory (98.9 vs 48.5 MiB). Both proxies saturate the 2-CPU container cap in
S-H1-02, so the CPU rows are near the cap and are not an efficiency claim.

The S-H1-01 and S-H1-03 raw rows are published for completeness only. They are
not evidence that HAProxy or Envoy is better or worse overall, and they are not
used to claim that “Envoy is slow”.

## 4. Integrity

The report declares **48 raw files**: 3 calibration files and 45 measured files
(3 scenarios × 3 products × 5 repetitions). Local verification against
`report.json` found:

```text
raw_files_actual=48
raw_files_declared=48
missing=[]
extra=[]
hash_mismatches=[]
run_counts=5 per scenario/product
non_clean_runs=[]
```

Every measured run returned only HTTP 200, with no entries in
`error_distribution` and `success_rate = 1.0`. The raw files retain the full
`oha` response-time histograms. The report checksum is
`1c0ff5770a5595b6e7fdd4b5ecb5f6971493be979eed0264b0860c8ae576d03e`;
`runner.log` is
`f901befafab3c6003ede70a77783dd2e7df8514ef938bd7da3964c954d59966a`.
The three configuration files have SHA-256 values:

```text
d945009ccacb82ff4e59f4f0873e62f1f6d1731bb2635fce1b4495563368c588  configs/s-h1-eqproxy.toml
03e4425d3d81b170348c74b5d277880b7d5f5e63f655d03848ccb0227a603ddc  configs/s-h1-haproxy.cfg
40f0e13b36aff0aa04413ea5e39fef98e88440c7207605efed0a51f9e0413d32  configs/s-h1-envoy.yaml
```

The report is 102,668 bytes; the raw directory is 126,108 bytes. The report's
`host_loadavg_end` is `7.75 6.69 4.03`; the start gate was applied at
`load1=0.17`, not after the run.

## 5. Reproduction

The harness and origin are maintainer-local (`scripts/` is local-only). The
final launch used a detached remote process so an SSH interruption could not
kill the measurement:

```bash
ssh netcup-bench 'cd /root/eqproxy-v092 && \
  OUT=/root/bench-v092-envoy-$(date -u +%Y%m%dT%H%M%SZ) && \
  mkdir -p "$OUT" && \
  setsid env MAX_LOADAVG=2.0 EQ_IMAGE=eqproxy-bench:v093 \
    OHA_BIN=/root/bin/oha-1.16.0 python3 -u scripts/bench_v092_envoy.py \
    --out "$OUT" --commit "9913a69+bench-v092-envoy-20261105" \
    > "$OUT.runner.log" 2>&1 < /dev/null &'
```

Evidence was copied back with `scp -r -p` and verified with the hash check
above. The EQProxy image was built on the host from the synced working tree, not
from a release artifact.

### Run history, including discarded attempts

- A 1-repetition, 2-second, 10-connection pilot was used to test the harness
  and was discarded; it is not evidence.
- One full launch was aborted before any measurement because the preflight saw
  `load1=4.95 > 2.00`; it produced no `report.json` and is not counted.
- One full launch was interrupted when the controlling SSH session ended
  before `report.json` was written; it left a partial remote directory and is
  not counted. The final launch used `setsid` and completed.
- The final run is `2026-11-05T22:04:05Z`, `48/48` raw files, with the results
  above. No failed or partial run was merged into this directory.

## 6. Limitations and NOT EXECUTED

1. **Only 3 of the 18 BENCHMARK_PLAN scenarios** are in this slice. It is not a
   new 18/18 claim; the earlier campaign remains the source for that count.
2. **S-H1-01 and S-H1-03 are Not comparable.** Their raw values are published
   but are not used for a winner. S-H1-03's direct-origin calibration is
   especially limited because the direct path opens a backend connection per
   request while the proxies reuse upstream connections.
3. **Operator study T-01…T-10: NOT EXECUTED.** The complete casilla, including
   this half, is deferred to v0.9.4. No participants were recruited, no sessions
   were run and no timings or questionnaire responses exist. The protocol and
   blank forms are in
   [`../v092-operator-study/README.md`](../v092-operator-study/README.md); they
   are a plan, not evidence.
4. One Linux host, one architecture (amd64), origin + proxies + generator
   sharing it; no CPU affinity, no separate machines and no NIC controls.
5. Closed-loop `oha` without coordinated-omission correction; full histograms
   are published. `perf` counters were not used.
6. Envoy is a **static bootstrap**, not Istio or xDS; no control-plane
   workload is measured. No TLS, HTTP/2, HTTP/3, rules, rewrites, compression,
   health checks or plugins are covered by this slice.
7. The EQProxy image is built from a working tree, not a published release
   artifact. The image ID identifies the code bytes used.
8. The other long-running containers on the host (`envoy-p1`, `prof-origin`,
   BuildKit) were not measured or limited by this campaign.
9. `n=5` per cell on one host is an exploratory measurement, not a market or
   production claim.

## 7. Files

```text
README.md             this report
report.json           disclosure, calibration, summaries, classification,
                      gaps, limitations and SHA-256 of every raw file
runner.log            console log of the final run
configs/              EQProxy, HAProxy and Envoy configurations
raw/                  48 oha JSON files (3 calibration + 45 measured)
```
