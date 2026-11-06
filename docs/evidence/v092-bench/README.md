# v0.9.2 benchmark slice — S-H1-04 and S-H1-05

Date: 2026-11-02 (UTC)
Status: **partial campaign; this slice covers 2 scenarios. With the later
S-H1-01/02/03, S-H2-01/02, S-TLS-01/02, S-TCP-01/02, S-H3-01/02, S-STR-01,
S-FAIL-01, S-CFG-01, S-OVL-01 and S-PLG-01 campaigns (only S-TLS-01 is certified) the roadmap is
18/18 with no benchmark scenario remaining;
item 16 stays open for the pending profiling/decisions.**

This directory covers only S-H1-04/05. The later H1 campaign on netcup and
Oracle is in [`../v092-bench-h1/`](../v092-bench-h1/), the H2 campaign is in
[`../v092-bench-h2/`](../v092-bench-h2/), and the later TLS campaign is in
[`../v092-bench-tls/`](../v092-bench-tls/).

This directory contains raw `oha` JSON, generated configs and the machine-readable
report for two scenarios of `BENCHMARK_PLAN.md`. It is not the complete plan and
must not be presented as such.

## Executed command

The image was built from the working tree at base commit `64a1d77` with
uncommitted changes (the tree is not a release tag):

```text
docker build --progress=plain -f docker/Dockerfile -t eqproxy-bench:v06 .
# exit 0
# image: sha256:3a5f6dc7ca6d762ba3753c34e84ef7845fef7b47f794705095c27f95e2a14237
```

The campaign then reused that fixed image:

```text
unset NO_COLOR
EQ_SKIP_BUILD=1 OHA_BIN=/Users/toni/.cargo/bin/oha REPS=5 OHA_SECS=10 CONNS=50 \
  python3 -u scripts/bench_v06.py \
  --out "$PWD/docs/evidence/v092-bench" \
  --commit "64a1d77+uncommitted-tree-20261102T215639Z"
# exit 0
```

`NO_COLOR` was unset for the generator only: `oha 1.14.0` reads that variable for
`--no-color` and rejects the value `1` before generating load. The first attempt
therefore produced no measurements; its log is
`target/audit-tools/logs/v092-bench-run.log` (local-only).

## Disclosure

- EQProxy: working tree based on `64a1d77`, image
  `sha256:3a5f6dc7ca6d762ba3753c34e84ef7845fef7b47f794705095c27f95e2a14237`.
- Reference: `haproxy:3.4.6`, image ID `e273915691a8`.
- Host: Apple M2 Pro, 10 logical cores, 32 GiB RAM, macOS 26.5.1 arm64, Docker
  29.9.2 (Linux aarch64 containers).
- Generator: `oha 1.14.0`, `-z 10s -c 50 --no-tui --output-format json
  --latency-correction`, one discarded warm-up per measured run.
- Topology: origins, EQProxy and HAProxy in containers on one Docker bridge;
  generator on the macOS host against published loopback ports. One host, not
  separate generator/proxy/backend machines.
- Repetitions: 5 per product and scenario; alternating product order per
  repetition (`eqproxy,haproxy` / `haproxy,eqproxy` / ...). Raw JSON is in
  `raw/`; `report.json` records SHA-256 and byte counts for all 20 files.
- Configs: `configs/s-h1-04-*.{toml,cfg}` and
  `configs/s-h1-05-*.{toml,cfg}`. Both products receive semantically equivalent
  routing, rewrite, headers and health-check settings.
- S-H1-04: 10 listeners, 100 backends, 20 rules; the measured request is
  `GET /bench/last` (19 misses, rule 20 match).
- S-H1-05: 1 listener, 1 backend with 500 origins and active HTTP checks.
- CPU is the sum of container process `utime+stime` over wall time; RSS/HWM are
  sampled from `/proc` after each run. Container base images differ (EQProxy
  Wolfi, HAProxy upstream image), so RSS is process memory, not image size.

## Results (median, min–max from five runs)

### S-H1-04

| Metric | EQProxy | HAProxy | HA/EQ ratio | Reading |
|---|---:|---:|---:|---|
| RPS | 9,959.2 (7,278.1–11,304.4) | 9,245.4 (7,222.6–11,167.0) | 1.077 | Comparable |
| P50 ms | 3.933 (3.706–4.761) | 4.178 (3.762–4.913) | 1.062 | Comparable |
| P95 ms | 10.409 (8.002–14.431) | 11.998 (8.699–18.784) | 1.153 | Comparable |
| P99 ms | 28.175 (21.178–46.236) | 27.365 (17.842–39.079) | 0.971 | Comparable |
| CPU % of one core | 126.0 (114.2–133.7) | 131.8 (115.2–135.6) | — | Comparable |
| RSS MiB | 28.6 | 56.6 | — | EQProxy lower |

### S-H1-05

| Metric | EQProxy | HAProxy | HA/EQ ratio | Reading |
|---|---:|---:|---:|---|
| RPS | 10,057.1 (5,561.2–10,286.7) | 8,734.9 (7,100.3–9,576.3) | 1.151 | Comparable |
| P50 ms | 4.059 (4.019–5.462) | 4.676 (4.377–5.569) | 1.152 | Comparable |
| P95 ms | 9.815 (9.104–24.522) | 12.174 (10.237–16.647) | 1.240 | Comparable |
| P99 ms | 24.927 (23.517–69.001) | 24.625 (20.568–30.855) | 0.988 | Comparable |
| CPU % of one core | 177.0 (174.6–198.9) | 174.5 (160.0–180.5) | — | Comparable |
| RSS MiB | 31.8 | 55.8 | — | EQProxy lower |

The classification follows `BENCHMARK_PLAN.md` §1.5: differences whose median
is inside the observed between-repetition spread are **Comparable**, not
**Better**. No metric was classified **Worse**. In particular, the EQProxy P99
point estimates are 2.9 % (S-H1-04) and 1.2 % (S-H1-05) above HAProxy, but both
ranges overlap; this is not promoted to a Worse verdict and therefore did not
trigger the profiling/ADR branch. The S-H1-05 EQProxy run 2 is a visible
outlier (5,561.2 RPS, P95 24.522 ms); it is retained in the raw data rather than
discarded.

## Limitations that keep item 16 open

1. Only **2 of 18** plan scenarios were executed **in this slice**. The later
   H1, H2 and TLS slices cover seven more; TCP, H3, streaming,
   failure/recovery, config-under-traffic, overload and plugins remain **NOT
   EXECUTED in this slice**.
2. The generator was **not calibrated against a local sink** and generator CPU
   was not measured; §5.1 is therefore not satisfied.
3. CPU affinity was not pinned and the host is macOS with Docker networking, not
   the Linux host used by the earlier canonical campaign. The plan's separate
   generator/proxy/backend topology was not available.
4. The `oha --latency-correction` flag is documented by `oha 1.14.0` as ignored
   unless `-q` is set; this run did not set `-q`, so the P50/P95/P99 values are
   **not coordinated-omission corrected**. Full raw histograms are published,
   and the limitation is recorded instead of claiming correction.
5. The benchmark used an uncommitted working tree based on `64a1d77`; it is not
   evidence about a published release artifact. The image digest above identifies
   the exact code bytes used.
6. No `perf`/flamegraph profiling was run because no metric met the Worse
   classification. This does not establish performance outside these two
   scenarios.

`report.json` is the machine-readable source for the tables and ratios; the
20 raw generator outputs are under `raw/`. SHA-256 values in `report.json` were
rechecked against the files: 20 files, 0 mismatches.
