# v0.9.2 — compression family in the differential corpus

**Objective.** Extend the shared-semantics corpus (roadmap 0.9.2, casilla 17)
with response compression, and prove the declared outcome against the three
real references. The cases observe the coding and the decoded payload, not
the compressor or the wire framing.

## Change

| Area | Change |
|---|---|
| harness | `http_request` now parses bytes; `chunked` bodies are dechunked and `content-encoding: gzip` bodies are gunzipped before comparison. Response-header assertions accept an empty declared value as **must be absent**, and the report prints the asserted header (or `<absent>`). |
| corpus | three cases: `z1-gzip`, `z2-no-accept-encoding`, `z3-identity` (`docs/differential-corpus.md`). |
| backends | every backend serves the same fixed `text/plain` payload under `/z/`, so all four proxies can be compared on one body. |
| HAProxy | `compression algo gzip` + `compression type text/plain`. |
| ingress-nginx | `gzip on` + `gzip_types text/plain` + `gzip_min_length 0` in the default server. |
| Envoy | `envoy.filters.http.compressor` with `min_content_length: 0` and `content_type: [text/plain]` in the main listener. |
| EQProxy | `[listener.web.compression]` with `gzip = true`, `min_size = 0`, `types = ["text/plain"]`. |

The payload is 213 bytes and contains no comma, so the existing comma
normalization cannot alter it. It is long enough to exceed HAProxy's minimum
response size; nginx, Envoy and EQProxy are explicitly configured with a zero
threshold.

## Decisions

1. **Compare the decoded payload.** The references legitimately differ on
   `Content-Length` vs `chunked` when they compress. The corpus declares the
   payload and decodes both the transfer and the content coding before
   comparing; it does not assert framing.
2. **Assert absence explicitly.** A body comparison alone cannot distinguish
   “identity” from “gzip that the harness decoded”. `z2`/`z3` declare an empty
   `content-encoding` value, which the harness reads as “header must be
   absent”; the report prints `<absent>` with the observed row.
3. **gzip and `text/plain` only.** Brotli is not uniformly available in the
   reference images, and per-type/minimum-size thresholds differ. One coding
   and one media type are enough to prove the shared negotiation; the rest is
   declared as a limitation, not skipped silently.
4. **No performance claim.** This increment proves negotiation and payload
   integrity, not ratio, CPU or latency. The compression benchmark remains
   `docs/evidence/v07-compression/bench.md`.
5. **Data planes, not controllers.** ingress-nginx is the `nginx` binary from
   the official controller image with a static config; Envoy uses a static
   bootstrap. The controller/xDS translation is outside this corpus.

## Execution

Command, run from the repository root on 2026-11-02:

```bash
cargo test -p eqproxy-cli --test differential -- --ignored --nocapture
```

Result: **exit 0**, `1 passed; 0 failed`, 16.88 s. Report:
[raw/differential-report.md](v092-differential-compression/raw/differential-report.md)
(SHA-256 `721c4eaceafecabaacbec8d3f5b3ff28e0188cbb97a334115b9af8373a6da67c`).
Full command output:
[raw/cargo-test.txt](v092-differential-compression/raw/cargo-test.txt).
Environment:
[raw/00-environment.txt](v092-differential-compression/raw/00-environment.txt).

| Item | Value |
|---|---|
| host | Darwin arm64, macOS 26.5.1 (25F80) |
| toolchain | `rustc 1.99.1` / `cargo 1.99.1` |
| Docker | client/server 29.9.2 |
| source | `main` at `6ba2667`; worktree at capture time: `differential.rs` modified and the evidence directory untracked. The corpus doc and this page were written after the run. |
| EQProxy binary | `target/debug/eqproxy`, SHA-256 `266296b2efaae23b9d79475d92eaa56b9a833619425beabc2c2f1b07450ba625` |
| HAProxy | `haproxy:3.4.6`, `sha256:e273915691a8eab3f105bf3e39fd4c29a4ecf334e33421903b5589d25e14db26` |
| ingress-nginx data plane | `registry.k8s.io/ingress-nginx/controller:v1.13.0`, `sha256:dc75a7baec7a3b827a5d7ab0acd10ab507904c7dad692365b3e3b596eca1afd2` |
| Envoy | `envoyproxy/envoy:v1.34.2`, `sha256:daca6a3f353ba289cc786d2162d13d4ec2b16d921c6c3f2fc57ce6f7900ab3d9` |
| backends | `nginx:alpine`, `sha256:a3554c0d0b48177b1413c26e01f695334428020d9df01243fbae3e11cfd17417` |

The harness is `differential_shared_semantics_corpus`; it runs the declared
corpus unchanged against the freshly built binary (native) and the three
reference containers, and writes the per-case report above.

## Observed results

The whole corpus is **26/26, 0 divergences, 0 exclusions**. The three new
rows, one column per implementation (EQProxy, HAProxy, ingress-nginx, Envoy):

| case | request | asserted observable | eqproxy | haproxy | ingress-nginx | envoy |
|---|---|---|---|---|---|---|
| `z1-gzip` | `Accept-Encoding: gzip` | `content-encoding` + decoded body | `gzip` | `gzip` | `gzip` | `gzip` |
| `z2-no-accept-encoding` | no `Accept-Encoding` | `content-encoding` absent + body | absent | absent | absent | absent |
| `z3-identity` | `Accept-Encoding: identity` | `content-encoding` absent + body | absent | absent | absent | absent |

In all twelve cells the decoded body is the 213-byte payload, and the status
is `200`. The report rows print the full payload and `content-encoding=gzip`
or `content-encoding=<absent>`, so the assertion is visible in the raw file,
not only in the verdict.

## Limits

- **Casilla 17 remains open.** This increment covers compression only; TLS,
  cache, error pages and limits are still pending, and the matrix is not
  reclassified by this page.
- **No release artifact or image was exercised.** The comparison uses the
  debug-profile binary from `CARGO_BIN_EXE_eqproxy` and reference containers;
  the release build and the published image are separate gates.
- **One payload, one coding, one type.** No Brotli, `HEAD`, q-value
  preferences, already-compressed responses, `Vary` or per-type thresholds.
- **HTTP/1.1 plaintext only.** No TLS, h2 or h3 in this family.
- **One host architecture.** Darwin arm64; the same run on Linux/amd64 is not
  claimed here.
- **ingress-nginx is a data plane.** The controller translation is not
  exercised, by the same decision that covers static Envoy.
