# v0.9.5 — release preparation evidence

Date: 2026-11-11. Branch `release/v0.9.5`, base `1a7bf12`. Tag `v0.9.5` is
annotated and points at the release commit that contains this file; the commit
is the one recorded in the final delivery and in Hindsight.

Binary: `target/release/eqproxy`, `eqproxy 0.9.5`,
SHA-256 `29abadc1ac0f43975544b42a77f19fec458e644e41136492e1efa0955fd3904d`.

## M-01

M-01 was already fixed in `24e6dc6` (`fix(net): close M-01 HTTP/1.0 framing for
v0.9.4`) and `v0.9.4` is an ancestor of this release tree. The final tree
re-runs its regressions:

- `cargo test -p eqproxy-net --lib chunked_transfer_encoding_is_rejected_in_http10`
  → **1 passed, 0 failed**.
- `cargo test -p eqproxy-net --test e2e_http1 http10_` → **2 passed, 0 failed**:
  `http10_client_gets_dechunked_close_delimited_response` and
  `http10_head_response_never_advertises_chunked_framing`.

M-01 is not an open blocker for v0.9.5.

## Gates on the release tree

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 0 — 799 passed, 0 failed, 2 ignored across 39 result blocks |
| `cargo build --workspace --release` | exit 0 |
| `git diff --check` | exit 0 |
| `cargo check --manifest-path fuzz/Cargo.toml --bins` | exit 0 |

The 39 result blocks are 36 test binaries plus 3 doctest blocks; the README's
"36 targets" is the binary count. The suite log is `/tmp/v095-suite-final.log`;
the gate logs are `/tmp/v095-final-fmt.log`, `/tmp/v095-final-clippy.log`,
`/tmp/v095-final-release_build.log` and `/tmp/v095-final-suite.log`.

## Release preparation

- `Cargo.toml` workspace version and the internal `eqproxy-*` pins are `0.9.5`;
  `Cargo.lock` and `fuzz/Cargo.lock` carry the same versions.
- `CHANGELOG.md` has `## [v0.9.5] — 2026-11-11` with AUD-01, AUD-02, ADV-01,
  ADV-02, AUD-03, the checkout liveness fix and the `rules` fuzz-target fix.
- `README.md` and the `docker/Dockerfile` example name v0.9.5.
- The tree has `v0.9.3` and `v0.9.4` as ancestors:
  `git merge-base --is-ancestor v0.9.3 HEAD` and
  `git merge-base --is-ancestor v0.9.4 HEAD` both exit 0.
- Tag/version and changelog extraction are checked with the release workflow's
  own shell/Python snippets; the result is recorded in the final delivery.

## Limits

No push, tag publication or workflow dispatch was performed. Docker Scout,
cross-architecture image E2E and the remote release jobs remain the user's
publication step; they are not claimed here.

**2026-11-12 update.** Publication was executed after this record was
written: the tag was set to `3451c19` (this tree plus the Wolfi base refresh
in `docker/Dockerfile`) and the GitHub release `v0.9.5` was published
(run `43232479817`). The paragraph above describes the state on 2026-11-11 and
is kept as run; the published-artifact facts and the checks executed against
them are in [the v0.9.5 release record](../release-0.9.5.md).

## CodeGraph impact addendum — 2026-11-14

This addendum evaluates the uncommitted `release/v0.9.5` working tree at
`ae85cf9`, after the v0.9.5 publication. It does **not** change the gates
table, the tagged-tree evidence or any published artifact. The evidence page
is [v095-codegraph-impact](v095-codegraph-impact.md): **18 of 110**
changed-symbol entries were queried (16.4 %), **92 are NOT EXECUTED**, and every
one of the 13 tracked source files has at least one query.

| Surface / risk | Affected parity rows | Verified source fact | Status in this addendum |
|---|---|---|---|
| `serve_http` | A2, A3, A4, A7; B-I1 and B-E2 share the h2/h3 paths | Shared HTTP/1.1 handler at `http1.rs:4345`; production callers `http2::exchange`, `http3::exchange`, `ws::serve` and `proxy::handle_connection`. The pre-fix baseline answer said the node did not exist; `rg` and the store probe refute that. | **SOURCE-VERIFIED**; no release E2E rerun: **NOT EXECUTED**. The pre-fix false absence is not a current PASS, and the supplement did not rerun this symbol. |
| `spawn_quic` | A4, D7 | Startup path `bind` → `make_entry` (`:1557`) and live-reload path `apply` (`:1785`, `KeptQuic::Adopt`), from `proxy.rs:1595`; the caller set matches `rg`. | **SOURCE-VERIFIED**; no reload E2E rerun: **NOT EXECUTED**. |
| `store_representation` | C12, B-N7, B-E4 (cache filter row) | One production consumer, `ResponseAssembly::run` (`http1.rs:4183`); `StoredRepresentation` shares that cache write path. | **SOURCE-VERIFIED**; no cache E2E rerun: **NOT EXECUTED**. |
| `declared_host` | A12 | Consumed only by `resolve_with` (`main.rs:2985`); it feeds backend TLS SNI, not listener routing (A13). The production `tls::client_config` call is at `main.rs:2986`, but CodeGraph does not record it. | **SOURCE-VERIFIED**; graph caller set incomplete; no TLS E2E rerun: **NOT EXECUTED**. |
| Listener knobs | D11, A17, B-N2, B-N6, B-N7, B-N19 | `report_secondary_listener_knob` has **9** call sites (`nginx.rs:1817`–`:2084`), not the 5 the answer reported: `proxy_cache`, `client_header_timeout`, `keepalive_timeout`, `proxy_connect_timeout`, `proxy_read_timeout`, `client_max_body_size`, `gzip`, `gzip_comp_level`, `gzip_min_length`. `multi_vhost` and `check_listener_addresses` caller sets are correct. | **SOURCE-VERIFIED**; no import E2E rerun: **NOT EXECUTED**. |
| Ingress HSTS classification | B-I10, D11 | `ingress::annotation` (`:519`) and `global_config` (`:844`) have caller `convert` (`:183`/`:317`); the HSTS branch now reports `review` instead of `done` when no `Strict-Transport-Security` header exists. Envoy `listener` and HAProxy `frontend` only add the mechanical `certs: Vec::new()` field. | **SOURCE-VERIFIED**; import-report classification only; no E2E rerun: **NOT EXECUTED**. |
| Coverage boundary | all | 92/110 entries, the four changed test files and the untracked test-only `security.rs` were not evaluated. | **NOT EXECUTED**; no row becomes a parity PASS from this addendum. |

The same row mapping is recorded in [the public parity matrix](../parity-matrix.md) §7.

This addendum is a risk map, not a new gate result. The existing row classes and
the release gate table above remain historical evidence from their recorded
runs; this review did not execute fmt, clippy, tests, build, Docker, Veyra or
Rust Audit. No PASS is declared for any new check.
