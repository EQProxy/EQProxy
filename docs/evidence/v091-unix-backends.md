# v0.9.1-b — Unix-domain backend sockets and per-server `source` bind

**Objective.** Close **B-H14 · B-N15** as [ADR-0043](../adr/0043-unix-sockets-and-source-bind.md)
adopted it: a backend server may be a local Unix-domain socket, and a TCP
server may bind its outbound socket to a chosen local address. The two
references spell the socket `proxy_pass http://unix:/run/app.sock` (nginx) and
`server app unix@/run/app.sock` (HAProxy). Nothing else changes: the admin
plane's socket stays [ADR-0021](../adr/0021-admin-unix-socket.md), and
transparent proxying stays E3 ([ADR-0039](../adr/0039-transparent-proxying-not-adopted.md)).

## 1. Configuration surface

| Knob | Values | Notes |
|---|---|---|
| `servers[].addr` | `IP:PORT`, `HOSTNAME:PORT`, `unix:/absolute/path`, `unix@/absolute/path` | The two Unix spellings normalise to the same target. Absolute, no NUL, ≤ 100 bytes (`MAX_UNIX_SOCKET_PATH`, [schema.rs:282](../../eqproxy-config/src/schema.rs#L282)). `unix@` is read from HAProxy sources only by the converter; the native schema accepts both |
| `servers[].source` | literal IPv4/IPv6 (`"10.0.0.5"`) | Binds the outbound socket before `connect`. A hostname is a validation error, not a deferred resolution |
| Listener `listen` | TCP only | A Unix *listener* is not part of this increment (ADR-0043 "Out of scope") |

`schema_version` stays `1`: both additions are additive and optional. The
example file and JSON schema document them
([eqproxy.toml.example](../../eqproxy.toml.example#L330),
[eqproxy.schema.json](../../eqproxy.schema.json#L1511)).

## 2. Semantics

- **Endpoint identity.** The runtime endpoint is now
  `Endpoint::Tcp(SocketAddr) | Endpoint::Unix(Arc<PathBuf>)`
  ([balancer.rs:212](../../eqproxy-net/src/balancer.rs#L212)). It is what the
  balancer picks, the keep-alive pool keys on
  ([pool.rs:70](../../eqproxy-net/src/pool.rs#L70)), the health checker probes
  ([health.rs:140](../../eqproxy-net/src/health.rs#L140)) and the access log
  renders: a TCP exchange keeps logging `server: "127.0.0.1:9000"`, a Unix one
  logs `server: "unix:/run/app.sock"` — the actual endpoint, never an invented
  address.
- **A Unix target is local.** No DNS (it is usable at the first pick, never a
  pending name), no TLS/SNI, no PROXY header, no HTTP/2 session (h2 is
  negotiated over TLS only). Validated at load, refused at dial as a second
  line of defence ([proxy.rs:2849](../../eqproxy-net/src/proxy.rs#L2849)).
- **Pooling.** The pool keys the idle connection by the Unix path exactly as it
  keys TCP by address; the second exchange on a fresh client connection reuses
  the parked socket (asserted in E2E).
- **Health checks** speak both TCP-connect and HTTP checks over the socket;
  the check's `Host` defaults to `localhost` when there is no address.
- **`source`.** The socket family follows the configured source (a v4 source
  cannot connect to a v6 literal, and that is reported, not coerced). The bind
  happens once per dial, inside `connect_timeout`. A bind failure is an
  explicit `502` with `connect_failures` incremented and its own log line
  ([proxy.rs:2976](../../eqproxy-net/src/proxy.rs#L2976)); there is no silent
  fallback to the kernel-chosen address.
- **Container note.** The published image runs unprivileged
  ([docker/Dockerfile](../../docker/Dockerfile#L79)); reaching a Unix backend
  from it needs the socket mounted into the container and permissions that
  allow that uid. That is a deployment step, not a code path, and it is not
  covered by this increment's evidence.

## 3. Validation gate

Three combinations cannot preserve their semantics and are configuration
errors with field, cause and example ([validate.rs:1615](../../eqproxy-config/src/validate.rs#L1615)):

| Combination | Message | Example offered |
|---|---|---|
| `proxy_protocol` + Unix target | `proxy_protocol cannot be sent to a Unix socket backend: there is no peer address to announce` | drop `proxy_protocol` |
| `tls` + Unix target | `TLS does not apply to a Unix socket backend: the connection never leaves the host` | drop `tls` |
| `source` + Unix target | `source binds a TCP socket and cannot apply to a Unix target` | drop `source`, or use a TCP `addr` |

A malformed `source` (hostname, empty) is rejected as well. The whole set is
exercised by `unix_targets_and_source_bind_choose_their_valid_combinations` and
by journey step 7 with the real binary.

## 4. Converter

| Source form | Classification | Emitted |
|---|---|---|
| nginx `proxy_pass http://unix:/run/app.sock` | converted | `servers = [{ addr = "unix:/run/app.sock" }]` |
| nginx `upstream` + `server unix:/run/app.sock;` | converted | same target in the upstream backend |
| HAProxy `server app unix@/run/app.sock` | converted | `unix:/run/app.sock` |
| HAProxy `server app /run/app.sock` | converted | `unix:/run/app.sock` (the bare path is the same target in the reference) |
| HAProxy `fd@12` | rejected with cause | — |
| `unix:` on a `listen`/`bind` | rejected with cause | — (a listener is still TCP only) |

The generated file is re-parsed and validated before it is written, and the
report names the conversion (`unix_backends_convert_in_both_reference_dialects`,
`addresses_accept_ips_hostnames_and_backend_unix_sockets`).

## 5. Journeys with the real binary

The journey is scripts/evidence/v091-unix-backends/journey.sh (local-only: `../../scripts/evidence/v091-unix-backends/journey.sh`)
(local-only harness, as `docs/` holds data only). It starts a real origin (a
Python `UnixListener`/`TCPListener`), the real `eqproxy` binary, and a real
`curl`, and asserts every step with `grep`; a failing assertion aborts the
script with a non-zero status.

### 5.1 macOS arm64 — release binary

Command: `BIN=target/release/eqproxy scripts/evidence/v091-unix-backends/journey.sh`
· exit **0** · artifact `target/release/eqproxy`, sha256
`dc3b1f4f3a4d50e052abf1b6885f3293b1469444de4a622208169cd9d792c161`, commit
`04859af`; `raw/00-environment.txt` records the same values. The journey was
re-run after the error-page loader bound fix (a CLI-only change outside the
Unix path); the Linux leg below was captured before that fix and is reported
as the earlier run.

| # | Journey | Observed | Raw |
|---|---|---|---|
| 1 | `mode = "http"` → `unix:/…/app.sock` | `HTTP/1.1 200 OK`, `Server: eqproxy`, body `from-unix-origin`; two exchanges answered | [raw/01-unix-http.txt](v091-unix-backends/raw/01-unix-http.txt) |
| 2 | `mode = "tcp"` → the same socket | body `from-unix-origin` through the tunnel | [raw/02-unix-tcp.txt](v091-unix-backends/raw/02-unix-tcp.txt) |
| 3 | missing socket path | `502` in 0.047 s, `could not connect to unix:…: No such file or directory (os error 2)`, `connect_failures=1` | [raw/03-missing-socket.txt](v091-unix-backends/raw/03-missing-socket.txt) |
| 4 | stale socket file (origin stopped, file left) | `502`, `Connection refused (os error 61)` | [raw/04-stale-socket.txt](v091-unix-backends/raw/04-stale-socket.txt) |
| 5 | `source = "127.0.0.1"` on a TCP server | `200`, origin reports `peer:127.0.0.1`; the bind is accepted, but on macOS that peer is also the default (see 5.2 for the discriminating case) | [raw/05-source-bind.txt](v091-unix-backends/raw/05-source-bind.txt) |
| 6 | `source = "192.0.2.1"` (TEST-NET-1) | `502` in 0.054 s, `could not bind source 192.0.2.1 … Can't assign requested address (os error 49)`, `connect_failures=1` | [raw/06-unusable-source.txt](v091-unix-backends/raw/06-unusable-source.txt) |
| 7 | `proxy_protocol` + `tls` + `source` on a Unix target | `config validate` exit **1**, three errors with field, cause and example | [raw/07-validate-forbidden.txt](v091-unix-backends/raw/07-validate-forbidden.txt) |
| 8 | nginx `proxy_pass http://unix:…` | exit **0**, `3 converted, 1 with notes, 0 rejected`, TOML carries `server = "unix:…"` | [raw/08-import-nginx.txt](v091-unix-backends/raw/08-import-nginx.txt) |
| 9 | the imported file validates | exit **0** (`config OK: 1 listener(s), 1 backend(s)`) | [raw/09-import-validate.txt](v091-unix-backends/raw/09-import-validate.txt) |

### 5.2 Linux arm64 (container, the production platform)

The same script, same commit, inside `rust:1.99.1` (`linux/arm64`, native on
this host), against the container-built debug binary (`/target/debug/eqproxy`,
sha256 in `raw-linux/00-environment.txt`).

- Exit **0**; the nine steps produce the same observable results
  ([raw-linux/](v091-unix-backends/raw-linux/)).
- **The discriminating source-bind case runs here**: Linux owns all of
  `127.0.0.0/8`, so the journey binds `source = "127.0.0.2"` and the origin
  reports `peer:127.0.0.2`. An unbound socket would have appeared as
  `127.0.0.1`, so this proves the bind changed the outbound address
  ([raw-linux/05-source-bind.txt](v091-unix-backends/raw-linux/05-source-bind.txt)).
  macOS only reserves `127.0.0.1` without a root-configured alias, which is why
  the discriminating address is recorded per platform and not hard-coded.
- A missing socket is `502` with `connect_failures=1` there too
  ([raw-linux/03-missing-socket.txt](v091-unix-backends/raw-linux/03-missing-socket.txt)).

## 6. Tests

`cargo test --workspace` on macOS arm64: **667 passed, 0 failed, 2 ignored**
(declared Docker harnesses), **32 test targets** — the 0.8.5 baseline was 652
in 31; the delta is the new E2E target plus the unit tests that came with the
feature.

New target `eqproxy-net/tests/e2e_unix.rs`, six E2E tests over **real sockets**
(a real `UnixListener` origin, the real proxy bind path, real TCP clients; the
only test double in the suite is still the DNS resolver in `e2e_dns.rs`):

- `tcp_listener_relays_to_a_unix_backend` — banner and echo both come from the
  Unix origin; one accept; byte counters exact.
- `http_listener_proxies_to_a_unix_backend_and_pools_the_connection` — two
  exchanges, **one** dial and **one** reuse: the pool keys the socket.
- `a_missing_unix_socket_fails_explicitly_and_fast` — 502 under 2 s,
  `connect_failures` 1, nothing opened.
- `source_binds_the_outbound_socket_to_the_chosen_address` — uses `127.0.0.2`
  where the platform has it (the peer can only be that address if the bind
  took effect) and degrades to the accepted-bind case on macOS.
- `an_unusable_source_is_an_explicit_connect_failure` — 502 fast, one counted
  failure, no silent fallback.
- `the_health_checker_speaks_http_to_a_unix_endpoint` — a 200 over the socket
  is UP; the dead socket (file left, listener gone) is DOWN.

Unit and contract tests: `unix_slots_are_usable_immediately_and_render_their_path`
(balancer slot, display, `unix_path`), `unix_targets_and_source_bind_choose_their_valid_combinations`
(parse and the three refusals), `addresses_accept_ips_hostnames_and_backend_unix_sockets`
and `unix_backends_convert_in_both_reference_dialects` (converter, both
dialects).

Review follow-up: `health::tests::health_connect_uses_the_configured_source_bind`
asserts that the health checker dials from the server's `source` too (a v6
source for a v4 target fails explicitly), so an operator firewall that admits
only the source address does not flap the server. `cargo test -p eqproxy-net
--lib health::` → **9 passed, 0 failed**.

## 7. Workspace checks

| Check | Command | Result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | exit 0 |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Suite | `cargo test --workspace` | exit 0 — 667 passed, 0 failed, 2 ignored |
| Release | `cargo build --workspace --release` | exit 0 |
| Linux E2E | container journey (5.2) | exit 0 |

## 8. Audits

The four legs answer different questions and none substitutes another. All
four were run for this increment.

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `/Users/toni/.veyra/.venv/bin/veyra audit . --profile standard` | **ACCEPTED**, exit 0: rustfmt, cargo check, clippy, cargo test, cargo-audit, anti-theater, nextest, llvm-cov, cargo-deny, cargo-machete and veyra-e2e OK. `dylint` and `cargo-semver-checks` **did not run** (declared: no lint config; unpublished crates have no baseline). `cargo-geiger` is **inconclusive**: it completed but reported coverage warnings in 4 runs, so its unsafe counts are a lower bound | `.veyra/evidence/20261031T231159Z/report.md` (local-only) |
| Rust Audit | `python3.13 <rust-ai-audit>/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | exit **1**: 0 critical / **7 high** / 402 medium / 109 low. The seven are the previously triaged ones — 3 R16 in `cache.rs` (a guard the heuristic believes crosses an `.await`, checked and refuted by reading the scope) and 4 R09 in `fuzz/fuzz_targets/config_import.rs` (panic is the reporting mechanism there) — **none introduced by this increment** | `report/rust-audit-20261101-000422.md` (local-only) + [v085-rust-audit-triage](v085-rust-audit-triage.md) |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-v091b.json`, then `agent impact` and `agent context` | exit **0**: 81 files, 78 440 lines, 8 588 nodes, 15 283 edges, `graph_complete: true`, `vector_index_ready: true`, 1 241 s. The claims used were checked against the source (see below) | `/tmp/cg-v091b.json` (ephemeral) |
| Hindsight | `hindsight-note --title "v0.9.1-b — Unix backends and source bind" --project EQProxy` | **stored**, exit 0 | bank `eqproxy` |

**What CodeGraph contributed, and what was verified.** The impact answer named
`select_server` ([proxy.rs:2020](../../eqproxy-net/src/proxy.rs#L2020)),
`pick_server` ([2039](../../eqproxy-net/src/proxy.rs#L2039)), the `Endpoint`
node and its impl ([balancer.rs:212](../../eqproxy-net/src/balancer.rs#L212)),
the pool key
([pool.rs:107](../../eqproxy-net/src/pool.rs#L107)/[139](../../eqproxy-net/src/pool.rs#L139)),
`connect_backend_to`
([proxy.rs:2413](../../eqproxy-net/src/proxy.rs#L2413)), `dial_transport`
([2827](../../eqproxy-net/src/proxy.rs#L2827)) and the four import sites
(`pool.rs:34`, `health.rs:33`, `http1.rs:41`, `lib.rs:44`); every one was
found at that location in the tree. Its caveat is real and kept: the graph
stores **no reverse edges on the `Endpoint` node** (Ca=0), so its list of
dependents is read from the consumers' own edges; 12 996 edges stay
unresolved (54 % resolution). The context answer's two `dial_with_retries`
line spans were reported truncated and were therefore not used. CodeGraph is a
map that was verified, not a witness of behaviour.

## 9. Limits

- A Unix **listener** is not supported; only backends (ADR-0043 out of scope).
- Abstract namespaces (`@name`) and `SOCK_SEQPACKET` are not supported.
- Filesystem permissions on the socket are the operator's: EQProxy connects as
  its own uid and reports `EACCES` as a connect failure like any other.
- `usesrc` / TPROXY (`source` with client-IP semantics) stays E3; this
  `source` only chooses the local address.
- The container image was not modified; mounting a host socket into it and its
  permissions are documented as a deployment step, not tested here.
- macOS cannot exercise the discriminating `source` address without a
  root-configured loopback alias; that half of the proof is the Linux leg.
- The macOS journey runs the **release** binary (5.1); the Linux leg runs the
  container-built debug binary (5.2). Both are the same commit; the release
  build is the artifact named in 5.1, not a proxy for it.
