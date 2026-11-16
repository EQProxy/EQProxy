# v0.10.0 — release preparation record

Language: **English**.

**What this page answers:** why this release is **0.10.0** and not the 0.9.6
patch it started as, which part of the public API forces that step, which gates
have actually run on this tree, and what remains unpublished. This page does
not move or create a tag and does not claim a publication.

- Version: **0.10.0**, from `[workspace.package] version` in `Cargo.toml`.
- Tag: **none for 0.10.0** — `git tag -l v0.10.0` returns nothing, locally or
  on the remote. The earlier local annotated tag `v0.9.6` still points at
  commit `0f03bf4`, whose `Cargo.toml` says `0.9.6`; it is not the tag for this
  tree and the release workflow would reject it
  (`v0.9.6` ≠ `v0.10.0` in the version-pin step).
- Current tree: the tip of `release/v0.9.5` after the three `EXT-H` fixes
  (`git log -1 --oneline`); nothing is pushed and no tag exists.
- Published release: **`v0.9.5`** (annotated tag `fd65bee` → commit `3451c19`,
  remote, published 2026-11-12), record in
  [release-0.9.5](release-0.9.5.md). Nothing was published from this tree.

## Why 0.10.0 and not 0.9.6

The release was prepared as a patch. `cargo-semver-checks` (Veyra leg,
`--baseline-rev v0.9.1`) reported five findings on the prepared tree, and one of
them is major-class:

| Finding | Item |
|---|---|
| `constructible_struct_adds_field` | `ListenerConfig.redirect` (`eqproxy-config/src/schema.rs:1149`) |
| `function_must_use_added` | `validate::check` (`eqproxy-config/src/validate.rs:41`) |
| `inherent_method_must_use_added` | `RateLimiter::new` (`eqproxy-net/src/proxy.rs:356`) |
| `inherent_method_must_use_added` | `RouteDecision::{rate_limiter, mirror, auth}` (`eqproxy-net/src/proxy.rs:473/485/497`) |

`eqproxy-core` needs no version change (223/223 checks pass). Four findings are
`#[must_use]` attributes the pedantic migration added; the fifth is the field
the redirect-only listener needs, and it breaks exhaustive struct literals.

Removing the attributes would not have kept the patch: the field alone is
major-class. Under the pre-1.0 convention that cargo-semver-checks itself
applies, the major-class step is the **middle digit**, so `0.9.1 → 0.10.0` is
the version step that absorbs the delta. Keeping 0.9.6 kept the leg red — that
is the run in `.veyra/evidence/20261116T183815Z/` — and the tool was right.

`veyra.toml` sets `release_type = "major"`: that is cargo-semver-checks'
vocabulary for the breaking class (its help for required updates spells it
`1.2.3 to 2.0.0 or 0.5.2 to 0.6.0`). Without the flag the tool derives exactly
the same change (`Checking eqproxy-config v0.9.1 -> v0.10.0 (major change)`) and
also passes; the explicit value keeps the run reproducible. Forcing
`release_type = "minor"` fails by construction, because a minor-class release
cannot carry a major-class change.

## Release claims

**M-01**, **P0-8**, the six `EXT-H` defects (`P0-10`…`P0-15`) and **N-2** are
claimed as closed fixes:

- **M-01:** HTTP/1.0 chunked framing remains fixed; the unit test and both
  E2E tests were executed red without the two HTTP/1.0 halves (exit 101) and
  green after restoring them. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
- **P0-8:** HAProxy `hdr(<name>)` and `hdr_dom(<name>)` preserve distinct
  header and port-free Host semantics, with parser, conversion and real-binary
  E2E coverage. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
- **P0-10 / EXT-H-01:** `cache::store_representation` decides
  `Content-Encoding`, `Content-Length`, `Vary` and the cache key in one place,
  so a stored entry cannot pair one representation's header with another's
  body. Regression in `eqproxy-net/src/cache.rs`; E2E in
  `eqproxy-net/tests/e2e_cache.rs`.
- **P0-12 / EXT-H-03:** the PROXY line is written on the raw TCP stream before
  the TLS handshake. `cargo test -p eqproxy-net --lib
  proxy_header_precedes_the_tls_handshake` and `cargo test -p eqproxy-cli
  --test cli_e2e backend_proxy_header_precedes_the_tls_handshake` were both
  executed red against the old order and green after the change.
- **P0-11 / EXT-H-02:** gRPC runs the WASM filter and `max_body_bytes`.
  `cargo test -p eqproxy-cli --test cli_e2e
  grpc_requests_obey_max_body_bytes_and_the_wasm_plugin` was red (`HTTP/2 502`,
  the bypass dialing the backend) and is green (`413` with `grpc-status: 8`,
  `403` with `grpc-status: 7`, neither dialing).
- **P0-13 / EXT-H-04:** `proxy_pass https://` is converted to TLS to the
  backend and a non-derivable case is reported as **revisar**. Corpus and
  real-binary coverage in `eqproxy-config/tests/import_corpus.rs` and
  `eqproxy-cli/tests/cli_e2e.rs`.
- **P0-14 / EXT-H-05:** the backend certificate is verified against the
  declared host (`BackendConfig::declared_host`), with the resolved address
  only as an explicit fallback. `cargo test -p eqproxy-cli --test cli_e2e
  run_verifies_the_declared_backend_hostname_not_its_ip`.
- **P0-15 / EXT-H-06:** `maxconn` is scoped to the accepting listener; the
  public gauge stays the sum. `cargo test -p eqproxy-core --lib
  maxconn_is_scoped_to_each_listener` and `cargo test -p eqproxy-cli --test
  cli_e2e maxconn_is_per_listener_not_process_wide`, the latter red against
  the process-wide counter. The QUIC path is exercised through the real binary
  too: `cargo test -p eqproxy-cli --test cli_http3_reload_e2e
  maxconn_is_per_listener_across_tcp_and_quic` runs a TCP listener at
  `maxconn = 1` and a QUIC listener at `maxconn = 2`, holds one served
  keep-alive connection on TCP and two real quinn `h3` handshakes on QUIC, and
  requires a third handshake to be refused with the QUIC `CONNECTION_REFUSED`
  close code while the admitted TCP connection keeps answering `200`.
  `eqproxy_connections_active` is read once, from the admin `/metrics`
  endpoint, after those admissions and must be `3` (1 + 2): the journey never
  polls the gauge and never relies on an idle timeout. With the per-listener
  keying reverted it fails with `CONNECTION_REFUSED` on the QUIC handshake
  caused by the TCP listener's connection.
- **N-2:** `Severity::Degraded` carries a mandatory typed reason and is
  reported in its own «revisar» section. Regression in
  `eqproxy-config/src/import/mod.rs`.
- **P1-9 / EXT-M-08, EXT-M-09:** the conversion report masks a credential
  instead of printing it, and the TOML and report are replaced by rename.
  `cargo test -p eqproxy-cli --test cli_e2e
  config_import_redacts_secrets_and_never_writes_half_a_file` and `...
  config_import_never_leaves_a_half_written_file` were red with the three
  files reverted (the report leaked the `Authorization` value; the destination
  kept its inode, i.e. it had been truncated in place) and are green now. The
  `--include-root` escape rejection (`EXT-M-15`) gained its regression.

P0-7 has no GitHub Actions run yet; `P1-1`…`P1-10` remain open. No P1 gate is
claimed, and the version step is an API-compatibility decision, not a feature
claim.

## Local gates on this tree (2026-11-16)

Run on the tip of `release/v0.9.5` after `P0-12`, `P0-11` and `P0-15`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | exit 0 — **883 passed, 0 failed, 2 ignored** across 41 result blocks |
| `cargo dylint --all` (`nightly-2026-05-28`) | exit 0 |
| `cargo semver-checks --baseline-rev v0.9.1 --release-type major` | exit 0 — `no semver update required` for `eqproxy-net` (0 checks run, 254 skipped: a major step accepts the delta by construction) |
| Release workflow version pin (`test "v$VER" = "$GITHUB_REF_NAME"`) | `VER=0.10.0`, exit 0 for a `v0.10.0` tag |
| Release workflow changelog extraction | finds `## [v0.10.0] — 2026-11-16` in `CHANGELOG.md` |
| Rust Audit gate (`bash .github/scripts/rust-audit-gate.sh`, `python3.13`) | exit 0 — 6 reviewed high findings, 0 new, 0 extra, 0 stale; `report/rust-audit-20261116-230917.md` |
| `check-secrets`, `check-evidence-test-citations`, `check-short-health-probes`, `check-docker-build-target`, `check-uid-skips` (each with `--self-test`) | exit 0 — 149 documents, 257 cited identifiers, 865 `#[test]` functions; 18 probes, 2 declared |
| `git diff --check` | exit 0 |
| `veyra audit . --profile standard` | exit 0 — **ACCEPTED**, 13 tools OK (`rustfmt`, `cargo check`, `clippy`, `cargo test`, `cargo-audit`, `veyra-anti-theater`, `nextest`, `llvm-cov`, `cargo-deny`, `cargo-machete`, `dylint`, **`cargo-semver-checks`**, `veyra-e2e`); `cargo-geiger` INCONCLUSIVE (4 dependency files unscanned — a lower bound, not a pass); 789.83 s in tools; evidence `.veyra/evidence/20261116T220825Z/` |
| CodeGraph `index . --batch-size 16 --stats-json /tmp/cg-index-0100.json` | exit 0 — 99 files, 101 551 lines, 11 110 nodes, 22 784 edges, `graph_complete` and `vector_index_ready` both true, `index_ms` 861 051 (~14 min). The first attempt aborted with `Source changed during indexing: ./eqproxy-cli/src/main.rs` because the tree moved under it, and the `--batch-size 64` retry failed with a SurrealDB write conflict; the cited index is the `--batch-size 16` run on the committed tree |

The `file:line` facts used from that index were re-resolved with `rg` before
being repeated: `Metrics::listener_counter` (`eqproxy-core/src/metrics.rs:851`),
`try_acquire_listener_connection` (`:868`) and the summed gauge write (`:878`),
the TCP gate (`eqproxy-net/src/proxy.rs:2164`) and its guard (`:3211`), the QUIC
gate (`eqproxy-net/src/http3.rs:93`) and its guard (`:121`), and the tests that
pin the behaviour (`metrics.rs:1033`, `cli_e2e.rs:11712`). The agent's `impact`
answer named `spawn_quic`/`open_quic` (`proxy.rs:1720`/`:1697`), both correct;
its `context` answer could not see the QUIC guard line, which `rg` places at
`http3.rs:121`.

The earlier run on the same tree before the version step
(`.veyra/evidence/20261116T183815Z/`) was **REJECTED** with
`semver-checks:semver-violation`; it is kept on disk and is the evidence that
the patch number was the problem, not the code.

## Missing for publication of 0.10.0

There is no `v0.10.0` tag, GitHub release, image digest, checksum, Docker Scout
gate or native amd64/arm64 journey for this tree, and the branch is not pushed.
`v0.9.5` remains the published release and does not cover these changes. The
tag/version pair will be consistent once `v0.10.0` is cut, because the workflow
reads the version from `Cargo.toml` and refuses any tag that does not match it.
The verdict is **NOT PRODUCTION READY**.
