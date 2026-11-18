# v0.10.0 — release preparation and publication record

Language: **English**.

**What this page answers:** why this release is **0.10.0** and not the 0.9.6
patch it started as, which part of the public API forces that step, which gates
actually ran on this tree, and what was published from it. Every published
number below was read back from the release, the registry and the run that
produced them.

- Version: **0.10.0**, from `[workspace.package] version` in `Cargo.toml`.
- Tag: **`v0.10.0`**, annotated and SSH-signed, object `fb56e28` → commit
  `bdf88bb` on `release/v0.9.5`, pushed to origin; it is the tree the release
  run built. GitHub reports that signature as `verified=false,
  reason=unknown_key` because the signing public key is not registered on the
  account as a *Signing Key*; `git -c
  gpg.ssh.allowedSignersFile=<file> tag -v v0.10.0` verifies it locally against
  `id_eqproxy.pub` (`Good "git" signature for toni with ED25519 key
  SHA256:CcbIhBxYmPf71LtEYCrMzAVOX/PtTdwwnvCTiTmR+WA`).
- Current tree: `bdf88bb`, exactly the commit the tag points at.
- Published releases: **`v0.10.0`** (2026-11-17T21:47:23Z; the
  [Publication](#publication--2026-11-18) section below) and, before it,
  **`v0.9.5`** (annotated tag `fd65bee` → commit `3451c19`, published
  2026-11-12), record in [release-0.9.5](release-0.9.5.md).

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

P0-7's workflow now runs from the default branch and from this branch
(`Fuzz` run `44124253752` green on `main` before the tag, `44119523205` green on
`release/v0.9.5`). The scheduled run of 2026-11-18 then found the
`convert_duration` overflow recorded under [the post-tag finding](#post-tag-finding-fuzz-overflow-in-convert_duration),
so P0-7 is **not** green. `P1-1`…`P1-10` remain open. No P1 gate is claimed,
and the version step is an API-compatibility decision, not a feature claim.

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

## Publication — 2026-11-18

The GitHub release
[v0.10.0](https://github.com/EQProxy/EQProxy/releases/tag/v0.10.0) was published
by the release workflow, run
[44183459466](https://github.com/EQProxy/EQProxy/actions/runs/44183459466)
(`success`, tag push; jobs 21:47Z…00:42Z). All five jobs are green: the line
gates, `Binaries linux (build + E2E verify) + SBOM`, `Multi-arch image (build +
Scout both archs + E2E + promote)`, `GitHub release (binaries + source tarball +
SHA256SUMS + SBOM + Scout)` and `Docker Hub page …`. Two earlier tag runs
failed, and each produced one of the two commits the tag now points at:

| Run | Failed gate | Fix |
|---|---|---|
| [44138009571](https://github.com/EQProxy/EQProxy/actions/runs/44138009571) | Docker Scout: `CVE-2026-84243` (high) in `ld-linux-2.44`, from `wolfi-base` glibc 2.44-r13 | `6de908c` repins the base to `sha256:e9b8c373…` (glibc 2.44-r21) in `FROM` and in the `base.digest` label |
| [44181874575](https://github.com/EQProxy/EQProxy/actions/runs/44181874575) | `eqproxy-config/tests/semver_contract.rs:29`: the baseline is `v0.10.0`, so the `enum_marked_non_exhaustive = "allow"` exception no longer matches | `bdf88bb` removes the exception and leaves the lint table empty |

The first of those runs is the one that proved the scanner was right: glibc
2.44-r13 carries a high-severity CVE that 2.44-r21 does not.

### Assets

Ten assets, all listed in `SHA256SUMS.txt` except the checksum file itself: the
two static Linux binaries, `eqproxy-0.10.0-src.tar.gz`, one CycloneDX SBOM per
crate (`eqproxy-core`, `eqproxy-config`, `eqproxy-net`, `eqproxy-cli`) and the
two `SCOUT-0.10.0-linux-*.md` reports. Verified from this macOS checkout on
2026-11-18 after `gh release download v0.10.0 -D /tmp/rel010`:

- `shasum -a 256 -c SHA256SUMS.txt` → **9/9 `OK`**;
- both binary assets executed in a container (`alpine:3.20`, `docker run
  --platform linux/amd64|linux/arm64 … --version`) → `eqproxy 0.10.0`;
- `docker run --platform linux/amd64|linux/arm64 ghcr.io/eqproxy/eqproxy:0.10.0
  --version` → `eqproxy 0.10.0` on both architectures.

### Image digests

- Index `ghcr.io/eqproxy/eqproxy:0.10.0` =
  `sha256:f8eee767f805c085bd624d847b6844aaa8d7436741354d9110b05d1a2013aea5`
  (`docker buildx imagetools inspect`). It is the digest the Scout gate scanned
  (`candidate index: …@sha256:f8eee767…`) and the one the promote step wrote to
  `ghcr.io/eqproxy/eqproxy:0.10.0`, `ghcr.io/eqproxy/eqproxy:latest`,
  `ghcr.io/eqproxy/eqproxy:0.10.0-candidate` and `exyonq/eqproxy:0.10.0` — the
  run logs `-> sha256:f8eee767… (scanned)` for each of them.
- Per-platform manifests: `sha256:a3f30263…` (linux/amd64) and
  `sha256:356e47f5…` (linux/arm64), the same digests printed inside
  `SCOUT-0.10.0-linux-amd64.md` and `SCOUT-0.10.0-linux-arm64.md`.
- Both Scout reports read `critical: 0`, `high: 0`, `medium: 0`, `low: 0`
  (69 packages, 21 MB amd64 / 18 MB arm64), and the gate's own lines are
  `scout: no critical or high vulnerability on linux/amd64` and `… linux/arm64`
  (`vulnerabilities │ 0C 0H 0M 0L`).

### Journeys in the run that published it

Read from run `44183459466`'s log, not re-run locally:

- `==> E2E ghcr.io/eqproxy/eqproxy:0.10.0-candidate: live traffic OK (image
  artifact, linux/amd64)` and the same line for `linux/arm64`;
- the admin-socket ACL journey prints `PASS` on both architectures for the
  socket owner/mode/type, the named ACL user, the group-class mask, the
  outsider refused by the kernel (`7 000`), the ACL edit across a reload, the
  clean shutdown and `no ERROR/WARN line in the server log`;
- the published tags were re-pulled and replayed on both registries and both
  architectures: `published ghcr.io/eqproxy/eqproxy:0.10.0 linux/amd64:</n  version, digest, h2 gate, config apply and live traffic OK`, and likewise for
  `linux/arm64`, `exyonq/eqproxy:0.10.0` amd64 and arm64.

**NOT EXECUTED here:** the native journeys on `netcup-bench` (amd64) and
`oracle-quasar` (arm64) — P1-1 — and anything that needs those hosts. The
workflow's gates are cited by their run and by the artifacts above; on this
macOS host only the `--version` checks and the checksum verification were
re-run.

## Post-tag finding: fuzz overflow in `convert_duration`

The scheduled `Fuzz` run on the default branch
([44235938866](https://github.com/EQProxy/EQProxy/actions/runs/44235938866),
2026-11-18T04:14Z) failed. `framing` and `rules` passed; `config_import`
aborted with `attempt to add with overflow` at
`eqproxy-config/src/import/mod.rs:752` — the `total_ms +=` in
`convert_duration` — from a hostile duration in the source configuration.
`value` comes
from `digits.parse::<f64>()`, so a figure such as `1e400` is `f64::INFINITY`,
the `as u64` cast saturates to `u64::MAX`, and the next part of the same
duration overflows. This breaks the fuzz target's own oracle (`convert` never
panics on hostile text), which is why the campaign is red; the input is
attached to that run as `fuzz-config_import-failure`
(`crash-4e019cfb443724a87b82b17493615f23edd2b7ff`).

It does **not** show up in the tagged release's gates: no `[profile]` in
`Cargo.toml` turns `overflow-checks` on, so the shipped 0.10.0 binary wraps the
total instead of panicking — a wrong duration, not a crash. Any
overflow-checked build (the fuzz target, `cargo test`, a debug build) panics.
The fix is a checked or saturating accumulation that turns the overflow into
the existing «duración no reconocida» review note; it is **not** in `v0.10.0`
and is the first item of the next patch release. Until it lands, the fuzz leg
on the default branch stays red and P0-7 is not green.

The release itself is published and its own gates are green; the production
verdict stays **NOT PRODUCTION READY** for the reasons on
[production-verdict](production-verdict.md): P1-1…P1-10 are open, this finding
is unfixed in the published tree, and no external operator has run it.
