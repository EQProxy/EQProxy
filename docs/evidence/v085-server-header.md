# The product banner — `Server: eqproxy`, never a version

**Objective.** Every HTTP response the dataplane sends carries exactly one
`Server: eqproxy`. The origin's value (often `nginx/1.24.0`, a CVE oracle) never
reaches the client, and the proxy's own build number is not advertised either:
a version in a header is a free fingerprint, and no operator migrates a proxy
because of its patch level.

**Change.** One normalizer and a handful of builders, all in the dataplane:

| Path | Where |
|---|---|
| constant | `eqproxy-net/src/http1.rs:90` (`SERVER_HEADER`) |
| head normalizer | `eqproxy-net/src/http1.rs:1709` (`with_server_header`) |
| proxied response, after every header source (h1, and every h2/h3 listener through the h1 engine) | `eqproxy-net/src/http1.rs:3291` |
| `101`/`CONNECT` switch head | `eqproxy-net/src/http1.rs:3232` |
| cached response and its `304` | `eqproxy-net/src/http1.rs:2113` |
| local `redirect`/`direct_response` | `eqproxy-net/src/http1.rs:1216` |
| auth deny | `eqproxy-net/src/http1.rs:1256` |
| local error (400/502/504/…) | `eqproxy-net/src/http1.rs:1118` |
| custom local reply (no server available) | `eqproxy-net/src/http1.rs:1170` |
| h2 stream refused before the engine | `eqproxy-net/src/http2.rs:335` |
| h3 stream refused before the engine | `eqproxy-net/src/http3.rs:952` |
| gRPC pass-through | `eqproxy-net/src/grpc.rs:325` |
| gRPC local answer/deny | `eqproxy-net/src/grpc.rs:646` |

## Decisions

1. **Replace, never append.** `Server` is a singleton; appending would leave the
   origin's version first and make the value depend on header order. The
   normalizer drops every existing `Server` line (case-insensitive) and inserts
   one right after the status line.
2. **No version.** `eqproxy`, not `eqproxy/0.8.5`. The header is stable across
   upgrades, so it cannot be used to pick a known-vulnerable build.
3. **Dataplane only.** The administration plane keeps its own responses
   unchanged: it is a management surface on its own listener/socket
   (ADR-0017), not a site served for third parties, and its JSON shapes are
   consumed by tooling. A `Server` line there would advertise the proxy to
   whoever already authenticated to it.
4. **`101`/`CONNECT` included.** The switch head is normalized before it is
   written, so an upgrade cannot smuggle the origin's banner. The proxy's own
   `100 Continue` interim response is written with no header fields at all; the
   final response of every exchange carries the banner.
5. **Cached entries are stamped as well.** A store written after this change
   persists the normalized head, and `serve_cached` normalizes again before
   writing, so an entry created before the change (or reloaded from
   `disk_path`) cannot ship the origin's banner either.

## Tests

Unit tests (`cargo test -p eqproxy-net --lib server_header` and
`… carries_the_product_server_header`), all passing:

- `server_header_replaces_the_origin_value_without_a_version` — `Server:
  nginx/1.24.0` becomes `Server: eqproxy`, a mixed-case `server: Apache/2.4`
  is replaced, a head without one gains exactly one, and a second pass is
  idempotent.
- `local_error_carries_the_product_server_header` — the 502 builder writes one
  line.
- `rule_response_carries_the_product_server_header` — the redirect builder
  writes one line even when a configured `response_headers` entry is named
  `server`.
- `response_header_actions_cannot_add_a_second_banner` — the proxied path's
  actual composition (`append_response_headers` then `with_server_header`)
  leaves one banner and keeps the other rule headers.

E2E with real sockets:

- `cargo test -p eqproxy-net --test e2e_http1 server_header_is` → 1 passed.
  The scripted origin answers `Server: nginx/1.24.0`; the client reads exactly
  one `Server: eqproxy`, no `nginx` substring, and the body is unchanged.
- `cargo test -p eqproxy-net --test e2e_cache miss_then_hit` → 1 passed. The
  origin's `ok` fixture now carries `Server: nginx/1.24.0`; both the `MISS` and
  the `HIT` serve `Server: eqproxy`.
- `cargo test -p eqproxy-net --test e2e_grpc grpc_forwarded_headers` → 1
  passed. The gRPC origin answers `server: grpc-origin/1.0`; the client sees
  `server: eqproxy`.
- `cargo test -p eqproxy-net --test e2e_resilience local_response` → 1
  passed. The custom local reply (`local_response`) carries
  `Server: eqproxy` exactly once, alongside its configured body.

## Journey with the released binary

Harness: `scripts/evidence/v085-server-header/journey.sh` (maintainer tree,
local-only). Raw transcripts: [raw/](v085-server-header/raw/).

```
uname: Darwin arm64 · rustc 1.99.1 (58cf8f9c3 2026-10-13)
binary: target/release/eqproxy
sha256: 5d81b436762f48b65c62bbab6786404af7041a09edc198ed2531f71d3ff2bbb8
version: eqproxy 0.8.5
config: docs/evidence/v085-server-header/raw/eqproxy.toml
```

The origin is a real `http.server` that answers `Server: nginx/1.24.0` with
`Cache-Control: max-age=60`; the listener has the response cache enabled and a
`/healthz` `direct_response`.

**Control — the origin's own banner:**

```
HTTP/1.0 200 OK
Server: nginx/1.24.0
Content-Type: text/plain
Cache-Control: max-age=60
Content-Length: 12
```

**HTTP/1.1 proxied** (origin body is `origin-body`):

```
HTTP/1.0 200 OK
Server: eqproxy
Content-Type: text/plain
Cache-Control: max-age=60
Content-Length: 12
x-cache: MISS
```

(The origin speaks HTTP/1.0 and EQProxy forwards that version; the banner is
independent of it.)

**HTTP/1.1 cached hit:** `x-cache: HIT`, `Server: eqproxy` — the stored
`nginx/1.24.0` is replaced at serve time.

**Local `direct_response`:** `HTTP/1.1 200 ok`, `Server: eqproxy`, body `pong`.

**HTTP/2 (h2c prior knowledge, `curl --http2-prior-knowledge`):**

```
HTTP/2 200
server: eqproxy
content-type: text/plain
cache-control: max-age=60
content-length: 12
x-cache: HIT
age: 0
```

(The h2 request hit the entry the h1 request stored; `h2-direct` on `/healthz`
also carries `server: eqproxy`.)

**Origin down:** `/after-the-origin` is `HTTP/1.1 502 bad gateway` with
`Server: eqproxy` and `content-length: 0`; the previously cached `/` still
serves `x-cache: HIT` with `Server: eqproxy`.

Exit status of the journey: **0**, final line `OK`
([journey.txt](v085-server-header/raw/journey.txt)).

## CodeGraph Rust on the touched slice

Index over the source tree after the last Rust edit (the later evidence-page
edits have no Rust nodes), `vector_index_ready: true`, `graph_complete: true`,
`complete: true`, `errors: 0`:

| Field | Value |
|---|---|
| `files` / `lines` | 80 / 77 145 |
| `nodes` | 8 438 |
| `edges` | 14 876 resolved, 12 882 unresolved (`resolution_rate` 53.6 %) |
| `embeddings` / `chunks` | 8 438 / 8 438 (`ollama`, 1 024 dims) |
| `index_ms` | 114 863 (1 min 55 s) build; 2 869 (2.9 s) final pass |
| `inference_texts` / `embedding_cache_hits` | 9 / 4 746 build; 0 / 0 final pass |

Raw stats: [build](v085-server-header/raw/codegraph/index-stats-build.json)
(the full pass that embedded the 9 changed chunks) and
[final](v085-server-header/raw/codegraph/index-stats-final.json) (the
consistency pass over the frozen tree; identical counts, nothing re-embedded).
Both agent queries exited 0; transcripts:
[impact](v085-server-header/raw/codegraph/agent-impact.txt),
[context](v085-server-header/raw/codegraph/agent-context.txt).

What the answers contributed, each checked against the source:

- It named the two production callers of `with_server_header` — `serve_http`
  and `serve_cached` — confirmed at `http1.rs:3291` and `:2113`.
- It flagged `write_no_server` (`http1.rs:1158`) as the one local branch that
  built a head without the banner. **Real gap, fixed**: the custom local reply
  now stamps it at `http1.rs:1170`, and `local_response_serves_the_configured_body`
  asserts it.
- It warned that `append_response_headers` appends blindly and that ordering
  against the normalizer decides whether a duplicate appears. **Real**: rule
  `response_headers` were appended after the first placement of the call. The
  stamp now runs at `http1.rs:3291`, after origin, rule actions, sticky cookie
  and CORS, and `response_header_actions_cannot_add_a_second_banner` pins it.
- It claimed h2/h3 responses "do not go through `with_server_header`" and read
  the `response_headers` unit test as evidence that h2 forwards the origin's
  value. **Wrong for the listener path**: the h2 and h3 listeners hand the
  exchange to `http1::serve_http` (`http2.rs:276`, `http3.rs:336`) and
  translate the resulting h1 head; the unit test asserts the raw translator,
  not a served response. The h2 journey above is the counter-evidence.
- It said the gRPC local path (`grpc.rs:691-715`) had "no explicit `server`
  insertion". **Wrong**: that function delegates to `local_answer`, which
  inserts it at `grpc.rs:646`; the agent read the caller and not the callee.
- It could not confirm whether `backend_h2::write_response_head`
  (`backend_h2.rs:938`) bypasses the banner. **It does not reach the client
  unnormalized**: `backend_h2::open` returns the pipes the HTTP/1.1 engine
  proxies from (`proxy.rs:2693`), so that head is parsed and re-emitted at
  `http1.rs:3291` like any other backend response.

## Audit legs for this increment

- **Veyra Proof** — `veyra audit . --profile standard`: **ACCEPTED**, exit 0,
  `.veyra/evidence/20261031T212408Z/` (macOS/aarch64, toolchain 1.99.1;
  `dylint` and `cargo-semver-checks` skipped with their declared reasons,
  `cargo-geiger` INCONCLUSIVE on 4 coverage warnings — not counted as OK).
- **Rust Audit** — 0 critical, 7 high, 397 medium, 107 low; the seven highs
  are the same ones the previous scan reported and are classified in
  [v085-rust-audit-triage](v085-rust-audit-triage.md). Exit 1 from
  `--fail-on high`, not from a new finding.
- **CodeGraph Rust** — index and two agent queries above.
- **Hindsight** — one note for this increment, in the `eqproxy` bank.

## What this does not prove

- **HTTP/3 was not exercised.** This machine's curl has `HTTP2` but not
  `HTTP3` (`curl --version` features), so there is no h3 client for a real
  run. The h3 listener's normal responses are the same h1 head translated by
  the same `http2::response_headers` code the h2 journey exercised, and its
  pre-engine refusals were given the header directly; that is code coverage,
  not an h3 run.
- **TLS listeners were not part of the journey** (plaintext h1 and h2c only).
  The header is written before protocol translation, so the TLS wrapper does
  not touch it, but no TLS run was made in this increment.
- **The admin API and panel are unchanged by design** (decision 3); no test
  asserts a `Server` line there.
- It does not prove absence of other version leaks (e.g. `Via`, `X-Powered-By`
  from an origin); those are outside this change.
