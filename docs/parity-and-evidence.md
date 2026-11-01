# Parity and evidence — what v0.9.1 ships and how it is proven

Language: **English**.

**What this page answers:** the detailed capability and evidence record that
used to open the README: what 0.9.1 adds, what the earlier lines added, the
measured performance including the losses, and the release gates. Moved from
the README on 2026-11-01; only link targets were adjusted for this directory.

## Release status

**Status: v0.9.1 (2026-11-01) — released and published; artifact gate met.
0.9.1 adds Unix-domain backends and per-server `source` bind,
custom error pages, the TLS version range and `errorloc`, the semantic
reference and the complete 120-row parity matrix. It builds on the v0.8.5
line: request-filter plugins as WASM with deny-by-default capabilities
(no WASI, no network, no filesystem to grant), a per-listener HTTP response
cache with `Vary`, ETag/304 revalidation and cache-lock, per-listener message
limits and stream buffers, hot TLS secrets in both directions, per-server
`maxconn`, and the official `eqproxy config import` converter for nginx,
HAProxy, ingress-nginx and Envoy — plus the v0.8 administration HTTP API with
roles, audit and a server-rendered web panel, hot configuration without a
control plane, per-server failover (`priority` ladders, `allbackups` pools,
`drain`/`maintenance`) and opt-in metrics, each with published raw
evidence.**
## Capabilities

A real TCP and HTTP/1.1 proxy (crates
`eqproxy-core/config/net/cli`) with **typed routing rules per listener** —
host/path/method/header/cookie/map/IP
conditions, first match wins — and four rule actions: `use_backend`,
`split` (weighted canary with `weight = 0` rollback, optionally pinned by
`hash_policy` so a key keeps its target, and optionally **mirrored** to a
shadow backend), `redirect` and `direct_response`. On top of the v0.5 dataplane (strict framing,
per-phase timeouts, **TLS termination and backend TLS, ALPN h1, mTLS,
multi-certificate SNI**, **active TCP/HTTP health checks, passive failure detection, safe
idempotent retries, bounded queues, PROXY protocol v1/v2,
hot config + certificate reload under traffic**, backend keep-alive pool):
**set/add/del request headers and response headers with `$remote_addr`/
`$host`/`$scheme` templates, weighted round-robin + leastconn + slow
start, source/cookie affinity, non-blocking dynamic DNS, canary by
header/cookie/weight, hot data maps updated without a config reload,
per-rule rate limiting, CORS/HSTS, basic + subrequest authentication,
WebSocket/`CONNECT` tunnels, `max_stream_duration`**, certificates selected
by SNI (exact name, then wildcard, then the listener default), and the `eqproxy`
CLI (`validate`, `explain` with per-rule trace and `timeout_on_fire`,
`schema` — the public JSON Schema for editor completion, `diff`, `plan`,
offline `apply`/`rollback`, `doctor`), backup server tiers (`backup = true`)
with a `priority` failover ladder and optional backup pooling
(`all_backups = true`, HAProxy's `option allbackups`), and an
**administration plane of its own**
([ADR-0017](adr/0017-read-only-admin-plane.md)): a bearer-gated
`GET /api/v1/status` + `GET /metrics`, hot backend/listener writes under
`If-Match` ([ADR-0019](adr/0019-hot-config-writes-via-admin-api.md)),
per-server `drain`/`maintenance`
([ADR-0018](adr/0018-first-write-path-server-state.md),
[ADR-0028](adr/0028-maintenance-closes-the-idle-pool.md)), certificate
renewal without a restart
([ADR-0025](adr/0025-certificate-renewal-without-restart.md)) and an
**opt-in server-rendered panel** with bearer sessions, a per-session CSRF
token and a write audit line, no JavaScript
([ADR-0029](adr/0029-panel-sessions-and-csrf.md)), with **715 tests
passed, 0 failed (2 declared `#[ignore]` Docker harnesses) across 36 targets**
(`cargo test --workspace`, 2026-11-01) plus two Docker-gated
harnesses (evidence from the v0.6 gate, not re-run since):
[differential corpus](differential-corpus.md) **23/23** cases against
HAProxy 3.4.6, nginx, the ingress-nginx data plane and static Envoy
v1.34.2, and the [migration matrix](migration/README.md) **65/65
checks** (four real source configs and their `eqproxy.toml` translation
routing identically; evidence in
[docs/evidence/v06-migration/](evidence/v06-migration/)). HTTP/2 is
terminated on client listeners and — when the backend TLS handshake
negotiates `h2` (`[backend.X.tls] alpn = ["h2"]`) — spoken to the backend
too, with **one session per backend server shared by concurrent
exchanges**, so an h2 client reaches an h2 origin through real HTTP/2 on
both sides and concurrent requests multiplex instead of queueing. Sessions
close when idle, a `GOAWAY` drains their in-flight streams, and a stream
the peer refused before processing is replayed once on a fresh session
(never after processing). **gRPC streams are relayed end to end**: both
directions stay open at once, `grpc-status` travels in trailers, routing,
auth and header rules apply through the same decision path, and
`idle_timeout`/`max_stream_duration` cut a stream explicitly instead of
hanging it. **WebSockets over HTTP/2** use an RFC 8441 extended CONNECT
(`:protocol = websocket`): the proxy translates the stream into the real
h1 handshake (with its own `Sec-WebSocket-Key`), answers the h2 client with
`200` (never `101`, and never the internal `Sec-WebSocket-Accept`) and
relays bytes with flow control as the only buffer; routing, auth and header
rules apply through the same h1 decision path, a refused upgrade is relayed
as a normal response, and a silent tunnel is cut by `idle_timeout`.
**HTTP/3 over QUIC** is served by a listener with `quic = true` and TLS, on
the same `IP:PORT` over UDP (TLS 1.3, ALPN `h3`, same certificate/SNI/mTLS);
each h3 request becomes one h1 exchange through the same engine, so routing,
auth, limits, header actions, mirroring, retries and compression decide
exactly once. Request bodies without a length stream upstream as chunked,
trailers cross in both directions, `CONNECT` answers `501`, QUIC connections
count in `maxconn`, and a TLS change on a live QUIC endpoint needs a restart
(the reload is rejected, never half-applied). Verified with a real `h3`
client and with two independent implementations, `aioquic` 1.2.0 and
`quic-go` 0.64.1, through the release binary.
Responses can be **gzip- or Brotli-compressed**
per listener when the client accepts it (streamed, never buffered,
negotiated from `Accept-Encoding`; wire bytes for both codings published in
[the raw benchmark](evidence/v07-compression/bench.md)). Request bodies
are **never decompressed**: a client's `Content-Encoding` is forwarded
untouched, so there is no decompression surface for a zip bomb
([THREAT_MODEL](../THREAT_MODEL.md) T09).
(design in [ADR-0006](adr/0006-http2-termination.md),
[ADR-0007](adr/0007-http2-backend.md),
[ADR-0008](adr/0008-http2-backend-multiplexing.md),
[ADR-0009](adr/0009-http2-session-lifecycle.md),
[ADR-0010](adr/0010-response-compression.md), evidence
[v07-http2](evidence/v07-http2.md),
[v07-http2-backend](evidence/v07-http2-backend.md),
[v07-http2-multiplexing](evidence/v07-http2-multiplexing.md),
[v07-http2-session-lifecycle](evidence/v07-http2-session-lifecycle.md),
[v07-compression](evidence/v07-compression.md),
[v07-grpc](evidence/v07-grpc.md),
[v07-ws](evidence/v07-ws.md) and
[v07-http3](evidence/v07-http3.md), with the soak in
[v07-grpc-soak-24h](evidence/v07-grpc-soak-24h.md); [ADR-0011](adr/0011-grpc-http2-passthrough.md)
covers gRPC, [ADR-0012](adr/0012-websockets-over-h2.md) covers
WebSockets over h2 and [ADR-0013](adr/0013-http3-quic.md) covers
HTTP/3 over QUIC).
The v0.8 administration surface has its own pages:
[authorization matrix](evidence/v08-endpoint-authorization.md),
[hot configuration](evidence/v08-admin-config-hot-apply.md),
[panel](evidence/v08-panel.md),
[maintenance window](evidence/v08-maintenance.md),
[priority ladder](evidence/v08-priority.md) and
[backup pool](evidence/v08-allbackups.md).
## Releases, benchmarks and soak

Release v0.9.1 (tag `v0.9.1`, release run 41299478887 green on all six
jobs, image index `sha256:95170d9a…` scanned 0 critical / 0 high on both
architectures and promoted unchanged to `:0.9.1` and `:latest` on GHCR and
Docker Hub — [artifact gate](release-0.9.1.md#51-result-2026-11-01);
binaries for darwin/linux amd64+arm64 and multi-arch image, see Install).
Release v0.8.5 (tag `v0.8.5`, release run 41165822327 green on all six
jobs, image index `sha256:bf1a06cd…` scanned 0 critical / 0 high on both
architectures and promoted unchanged to `:0.8.5` and `:latest` on GHCR and
Docker Hub — [artifact gate](evidence/v085-published.md); binaries for
darwin/linux amd64+arm64 and multi-arch image, see Install). Performance vs 3.4.6 on netcup-bench (amd64, medians
of 5): the v0.6 HTTP/1.1 scenarios S-H1-04 (10 listeners, 100 backends,
20 rules) **18 207 vs 15 957 rps** (p95 4.45 vs 5.92 ms) and S-H1-05
(500 backends + health checks) **15 624 vs 3 466 rps** (p95 5.16 vs
31.4 ms) ([evidence](evidence/v06-bench/)), with the arm64 campaign
([evidence](evidence/v06-bench-arm64/)) leading in both and S-H1-04
gaps within noise. The v0.7 gate scenarios, with h2/h3 active on both
sides, are published including the losses: S-H2-02 **1.081×** (Better),
S-H2-01 **0.742×** (Worse, up from 0.099× once the pool stopped churning),
and with HTTP/3 active S-H3-01 **0.668×** and S-H3-02 **0.768×**, both
Worse ([pool sizing](evidence/v07-h2-pool-sizing.md),
[offload](evidence/v07-h3-profile.md),
[close fix](evidence/v07-h3-close.md)).

## Scope and non-goals

**Scope honesty:** v0.9.1 is not declared production ready (Phase 9 gate
pending); its roadmap is closed at 13/13 and its publication gate is met:
annotated tag over the release commit `e7473ea`, all six tag-run jobs green,
the scanned index promoted unchanged, and the published image re-pulled and
exercised ([artifact gate](release-0.9.1.md#51-result-2026-11-01)).
The v0.8.5 release was not declared production
ready either. Its own gate was met: 22/22 boxes of the v0.8.5 roadmap, including
the artifact gate — tag == `Cargo.toml`, every binary and both image
architectures built with `1.99.1`, the scanned digest promoted unchanged, and
the published image re-pulled and replayed
([published](evidence/v085-published.md)). It keeps the v0.8
administration guarantees: the route×role authorization matrix with the
dataplane serving when the admin task dies, a backend applied under traffic
in under a second, and the metrics budget refusing what exceeds it. The
latest 24-hour soak remains
the v0.7 gRPC bidi run **PASS** (2026-10-23: 86 400 s, 4 267 messages, 0 broken
streams, 0 failed probes), and HTTP/3 interop with two independent clients
(`aioquic` 1.2.0 and `quic-go` 0.64.1) is unchanged. HTTP/3 stays **experimental**:
browsers and `curl` over h3 are unverified and the QUIC adversarial tests
of THREAT_MODEL T19 are not executed. Outside this version by design and
listed in the [CHANGELOG](../CHANGELOG.md): FastCGI, uwsgi and scgi
([ADR-0032](adr/0032-fastcgi-uwsgi-scgi-decision.md)), the cache's
`stale-while-revalidate` / `stale-if-error`
([ADR-0034](adr/0034-http-response-cache.md)), and maps consumed through
`equals` matching only.
The v0.6 24h soak gate **passed** on
netcup-bench (2026-10-18: 17.28M requests, 0 errors, steady RSS
8.86 MiB / peak 9.32 MiB, clean SIGINT shutdown; budgets in
[BENCHMARK_PLAN.md](../BENCHMARK_PLAN.md) §9).
