# EQProxy

EQProxy — the reverse proxy and load balancer that imports your HAProxy and nginx config, and proves the translation routes the same.

[![Releases](https://img.shields.io/badge/releases-all-blue)](https://github.com/EQProxy/EQProxy/releases)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-MIT)
![Status](https://img.shields.io/badge/status-pre--1.0-orange)

**65/65 migration checks** against HAProxy 3.4.6, nginx, ingress-nginx and
static Envoy 1.34.2 · **31/31** differential cases · **738 tests** across 36
targets · a single static Linux binary (musl).

```bash
# 1. Translate the config you already have. The report names every directive
#    that was converted, needs review or was refused.
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/w" -w /w \
  exyonq/eqproxy:latest \
  config import --input haproxy.cfg --output eqproxy.toml --report report.md

# 2. Validate offline (binds nothing). `config explain` traces a request when you need it.
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/w" -w /w \
  exyonq/eqproxy:latest config validate --config eqproxy.toml

# 3. Run it.
docker run --rm -p 8080:8080 -v "$PWD/eqproxy.toml:/etc/eqproxy.toml:ro" \
  exyonq/eqproxy:latest run --config /etc/eqproxy.toml
```

`config import` exits **0** on a clean conversion, **2** when the file is valid
but directives were refused (the report names them), and **1** when nothing was
written.

## Why you'd try this

**You keep your config.** `config import` reads nginx, HAProxy, ingress-nginx
and Envoy, classifies every directive as converted / to-review / rejected, and
writes a report. A silent half-translation is worse than a refusal, so the
converter refuses what it cannot map and says so.

**The translation is proven, not claimed.** A Rust harness starts the four
originals in containers and the `eqproxy` binary natively, all in front of the
same five echo backends, sends 13 routing cases to each and asserts identical
status, body and headers: **65/65** checks. The header corpus is another
**23/23**, with all four implementations asserted and no exclusions.

**It tells you why.** `eqproxy explain` traces a request through the rule set
per listener (`timeout_on_fire` included); `config schema` ships the JSON
Schema for editor completion; `doctor`, `plan`, `diff`, `apply` and `rollback`
cover the operational path.

## Status

**v0.9.2 · pre-1.0 · not production-ready.** The test suite is green across 36 targets,
real HTTP/1.1, HTTP/2 and HTTP/3 dataplanes — but no users outside the author
yet. Try it on a staging config and tell me what breaks:
[KNOWN_GAPS.md](KNOWN_GAPS.md) · [open an issue](https://github.com/EQProxy/EQProxy/issues).

## What's in it

Typed per-listener routing rules (host/path/method/header/cookie/map/IP, first
match wins) with four actions — `use_backend`, `split` (weighted canary,
`weight = 0` rollback, hash pinning, mirroring), `redirect` and
`direct_response`; TLS, mTLS and SNI; HTTP/2 termination and backend h2; HTTP/3
over QUIC; gRPC; WebSocket and CONNECT; gzip/Brotli; a per-listener response
cache; health checks; failover with `priority` ladders and backup pools; hot
reload without a control plane; an admin API with roles and audit; opt-in
metrics; and a server-rendered panel with no JavaScript.

## Install

Binaries and checksums live on the
[release page](https://github.com/EQProxy/EQProxy/releases/latest). Every
release carries two prebuilt Linux binaries — `eqproxy-linux-arm64` and
`eqproxy-linux-amd64` — the matching sources as
`eqproxy-<version>-src.tar.gz`, the SBOM of each crate and
the Docker Scout report of both image architectures, all checked by one
`SHA256SUMS.txt`. Both images must pass
`docker scout cves --only-severity critical,high --exit-code` on
`linux/amd64` and `linux/arm64` at **0 critical / 0 high** before the release
can be promoted; the reports are attached to the release.

The Linux binaries are **statically linked against musl** (`file` reports
`static-pie linked` on amd64 and `statically linked` on arm64, no `NEEDED`
entries, no interpreter), so they need no libc, no glibc version and no
packages: they run on any Linux distribution of that architecture, and on
`FROM scratch`. There is no prebuilt Darwin asset; macOS builds from source
with `cargo build --release --locked`.

```bash
# Pick your architecture:
#   eqproxy-linux-arm64 | eqproxy-linux-amd64   (static, musl)
curl -sSLO https://github.com/EQProxy/EQProxy/releases/latest/download/eqproxy-linux-amd64
curl -sSLO https://github.com/EQProxy/EQProxy/releases/latest/download/SHA256SUMS.txt
shasum -a 256 -c SHA256SUMS.txt --ignore-missing   # Linux: sha256sum -c --ignore-missing SHA256SUMS.txt
chmod +x eqproxy-linux-amd64
./eqproxy-linux-amd64 --version
```

Docker (linux/amd64 + linux/arm64, Wolfi base), same digests on both registries:

```bash
docker pull exyonq/eqproxy:latest           # Docker Hub
docker pull ghcr.io/exyonq/eqproxy:latest   # GitHub Container Registry
docker run --rm exyonq/eqproxy:latest --version
```

The examples use the moving `latest` tag; pin a release tag when you need a
reproducible pull.

## Docs

| Document | Contents |
|---|---|
| [Migration matrix](docs/migration/README.md) | 65/65: four real source configs and their translation |
| [Differential corpus](docs/differential-corpus.md) | 31/31 against HAProxy, nginx, ingress-nginx, Envoy |
| [SPEC.md](SPEC.md) · [THREAT_MODEL.md](THREAT_MODEL.md) | Goals, architecture, non-protections |
| [docs/semantics.md](docs/semantics.md) | Per-feature semantics, limits and what EQProxy does **not** do |
| [ADRs](docs/adr/) | One file per design decision |
| [Benchmarks & raw evidence](docs/evidence/) | Every number in this README ships its log |

Documentation is published in English only.

## Parity & evidence

The long version — every capability, the release that introduced it and the raw
evidence behind each number — is in
[docs/parity-and-evidence.md](docs/parity-and-evidence.md). The headline gates:

- **Migration:** 65/65 checks (13 cases × 5 implementations) against HAProxy
  3.4.6, operator nginx, ingress-nginx and static Envoy 1.34.2.
- **Differential:** 23/23 header cases, all four columns asserted, no
  exclusions.
- **Scope:** all 120 reference rows classified E1 (differential), E2
  (documented difference) or E3 (ADR) in
  [docs/parity-matrix.md](docs/parity-matrix.md).
- **Performance vs 3.4.6** (netcup-bench amd64, medians of 5): S-H1-04
  **18 207 vs 15 957 rps**, S-H1-05 **15 624 vs 3 466 rps**
  ([benchmark evidence](docs/evidence/v06-bench/)); the HTTP/2 and HTTP/3 gate
  scenarios publish the losses too — S-H2-01 **0.742×**, S-H3-01 **0.668×**, S-H3-02 **0.768×**
  ([pool sizing](docs/evidence/v07-h2-pool-sizing.md),
  [h3 offload](docs/evidence/v07-h3-profile.md)).
- **Release:** the release workflow builds and E2E-verifies the binaries, scans
  both image architectures with Docker Scout at **0 critical / 0 high**, and
  publishes the SBOM, Scout reports and `SHA256SUMS.txt` with the tagged
  release.

## Key decisions

- **Comparison reference:** versions 3.4.6 LTS (primary) + 3.0.29 LTS (legacy
  deployed) of the reference proxy. See
  [ADR-0003](docs/adr/0003-multiversion-reference.md).
- **Production platform:** Linux x86_64 (glibc). See
  [ADR-0001](docs/adr/0001-production-platform.md).
- **Configuration format:** one TOML format behind a typed schema, decided in
  [ADR-0002](docs/adr/0002-configuration-format.md) (**Accepted** 2026-10-12)
  and implemented (parser, semantic validation, versioned `schema_version`).
- **Network stack:** tokio + rustls, this project's own strict HTTP/1.1 parser
  and the `h2` crate for HTTP/2, quinn + h3 for HTTP/3 — **no hyper**: the
  dataplane forwards the exact bytes it validated and refuses any framing with
  two readings ([ADR-0036](docs/adr/0036-own-http1-parser.md)). HTTP/3 is
  **mandatory scope** (Phase 5; two-client interop passed on 2026-10-23, still
  experimental pending the QUIC adversarial tests). See
  [ADR-0004](docs/adr/0004-network-stack-and-crate-layout.md).
- **Performance goal:** match or beat the reference proxy 3.4.6 LTS on
  connections, memory,
  CPU and P50/P95/P99 latencies in the scenarios of
  [BENCHMARK_PLAN.md](BENCHMARK_PLAN.md). First measurements
  (S-TCP-01/S-H1-01/S-TLS-01, [evidence](docs/evidence/phase2c/bench.md)):
  **Comparable** after the A16 keep-alive pool; the rest of the matrix is
  **Not measured** (no data, no claim).
- **Code:** 100 % safe Rust (`#![forbid(unsafe_code)]`, no exceptions);
  extensions WASM-only (Lua discarded due to FFI `unsafe`).
- **Parity scope:** the OSS edition of the reference proxy only.
- **Migration:** configuration converter from the reference proxy to EQProxy
  (CLI `eqproxy config import`) with its own parser for the reference
  configuration format and a conversion report; directives are never silently
  dropped.

## License

Dual: [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE), at the recipient's
choice.

---

A product of ExyonQ.
