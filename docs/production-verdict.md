# Production verdict — EQProxy v0.10.0

**Verdict: NOT PRODUCTION READY.**

This page is the production verdict for the v0.10.0 hardening tree. It separates
what has actually been executed from what is still a hypothesis, a local-only
result or a known open defect. A green local build is not a production
declaration.

## Revision and artifact

| Item | State |
|---|---|
| Branch | `release/v0.9.5`, pushed; `HEAD` = `bdf88bb` = the commit `v0.10.0` points at |
| Source revision | `bdf88bb` — the tag target. It contains the 0.10.0 preparation, the six `EXT-H` fixes, `N-2`, `P1-9`/`EXT-M-08`/`EXT-M-09`, and the two commits the failed release runs produced (`6de908c`, `bdf88bb`); `v0.9.5` was published from `3451c19` |
| Version | `0.10.0` in `Cargo.toml`; `CHANGELOG.md` has `## [v0.10.0]` |
| Tag | **`v0.10.0`**, annotated and SSH-signed, object `fb56e28` → `bdf88bb`, remote. GitHub shows `verified=false, reason=unknown_key` until that public key is registered on the account as a *Signing Key*; local `git tag -v` verifies it. `v0.9.5` is annotated `fd65bee` → `3451c19`, published 2026-11-12 |
| Release claims | **M-01**, **P0-8**, the six `EXT-H` findings (**P0-10**…**P0-15**), **N-2** and **P1-9**/**EXT-M-08**/**EXT-M-09**, each with executed evidence in [release-0.10.0](release-0.10.0.md). **P0-7** is not green (below); P1-1…P1-10 remain open |
| Published artifact | **`v0.10.0`**, GitHub release 2026-11-17T21:47:23Z from run [44183459466](https://github.com/EQProxy/EQProxy/actions/runs/44183459466) (`success`), plus the multi-arch image index `sha256:f8eee767f805c085bd624d847b6844aaa8d7436741354d9110b05d1a2013aea5` on `ghcr.io/eqproxy/eqproxy` and `exyonq/eqproxy` (`:0.10.0`, `:latest`) |
| Final local suite | `cargo test --workspace` → **883 passed, 0 failed, 2 ignored** on `bdf88bb`, re-run 2026-11-18; fmt, clippy `-D warnings` and `cargo dylint --all` exit 0; `cargo semver-checks --baseline-rev v0.10.0 --release-type major` exit 0; see [release-0.10.0](release-0.10.0.md) |

## What the evidence actually shows

- **Workspace suite:** `cargo test --workspace` on the tag target `bdf88bb`
  ran **883 passed, 0 failed, 2 ignored**, re-run on 2026-11-18 after the last
  release fix (`cargo semver-checks --baseline-rev v0.10.0 --release-type major`
  exit 0 on the same tree). The first 0.10.0 record measured 873 passed in 41
  result blocks on 2026-11-16 (`.veyra/evidence/20261116T201501Z/`) and the
  P0-8/P0-9 tree (`0f03bf4`) 807 in 39 blocks; the difference is the v0.9.6
  import work — importer parser and conversion, the corpus fixture move and the
  real-binary E2E — plus the HTTP/3 hot-reload E2E and the tests the `EXT-H`
  fixes added. Historical detail: [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
- **M-01 framing regression:** the fix is present in `http1.rs`; the unit test
  `chunked_transfer_encoding_is_rejected_in_http10` and the two
  `http10_` E2E tests are executed in the final suite. A destructive check
  removed both HTTP/1.0 halves: the unit test and both E2E tests went red
  (exit 101), then returned green after restoring the file; the exact output is
  in [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md). This demonstrates the
  HTTP/1.0 rewrite, not a general proof for arbitrary clients.
- **Adversarial properties:** `cargo test -p eqproxy-net --test
  adversarial_properties` → **11 passed, 0 failed** (deterministic PRNG).
- **Adversarial lab:** the historical release-binary run is **14/14 PASS**
  (S9, S10, S12 h2 and S12 h1), with raw results in
  `docs/evidence/v095-p0-3-p0-4-results.json`. The harness moved to
  `scripts/evidence/adversarial-2026-11-09/run_lab.py` and was **not rerun**
  during the documentation move.
- **Fuzzing:** three seeded 60-second campaigns (`framing`, `rules`,
  `config_import`) completed without crashes and found one real importer-report
  defect that was fixed. A short campaign is evidence of no crash in that
  budget, **not** proof of absence of defects; longer campaigns remain P1-2.
- **Trailers:** the 64 KiB/128-field budget and the real 100 000-trailer RSS
  measurement are recorded in `docs/evidence/v095-p0-5.md`. The h3 path is not
  covered by a QUIC E2E for this limit.

## What is open or not verified

### Release and delivery

- **P0-7 / remote CI — executed.** The workflow runs from the default branch:
  `44124253752` green on `main` (2026-11-17), `44119523205` green on
  `release/v0.9.5`, and `44324110633` green on `main` at `513f6c9`
  (workflow_dispatch, three targets, `config_import` 21 373 executions). The
  scheduled run of 2026-11-18 (`44235938866`) is the one that failed:
  `config_import` aborted on `attempt to add with overflow` in
  `convert_duration`'s `u64` accumulation, with the crash input attached
  (`fuzz-config_import-failure`, `crash-4e019cfb…`). Root cause: a 22-digit
  figure does not fit the millisecond total, the `as u64` cast saturates to
  `u64::MAX` and the next part overflows — a panic under overflow checks, a
  wrapped duration without them. Fixed on `main` in `513f6c9` with a
  regression test that reproduces that artifact. The published `v0.10.0` was
  built from `bdf88bb`, before the fix, and still carries it.
- **P1-1 / architectures:** no native `linux/amd64` journey on `netcup-bench`
  and no native `linux/arm64` journey on `oracle-quasar` for this tree. The
  container journeys the release workflow runs on both architectures are
  executed and green ([release-0.10.0](release-0.10.0.md)); they are not those
  hosts.
- **Docker / Scout — executed.** The multi-arch image was built, scanned and
  promoted by run `44183459466`: both architectures report
  `critical: 0`, `high: 0`, `medium: 0`, `low: 0` (`vulnerabilities │ 0C 0H 0M
  0L`), the gate exits 0 on both, and the promoted tags carry the scanned index
  `sha256:f8eee767…`.
- **Publication — executed.** `v0.10.0` is a GitHub release with ten assets,
  `SHA256SUMS.txt` covering nine of them (9/9 `OK` from a fresh download), both
  binaries and both image architectures reporting `eqproxy 0.10.0`, and the
  published tags re-pulled and replayed on two registries × two architectures.
  The tag signature is locally valid but unverified on GitHub (`unknown_key`).
- **P1-4:** closed for `0.10.0` — version, changelog, signed tag, publication,
  checksums and image digests are consistent and read back from the release.
  The release is not a production declaration.

### Open defects

The six external high-severity findings are **fixed and E2E-verified in the
published tree**, each by a named command that was executed red before the fix
and green after it:

| ID | Defect | State |
|---|---|---|
| EXT-H-01 / P0-10 | Compressed cache HIT mixes compressed headers with an identity body | Fixed; regression in `eqproxy-net/src/cache.rs`, E2E in `eqproxy-net/tests/e2e_cache.rs` |
| EXT-H-02 / P0-11 | gRPC bypasses the WASM plugin and `max_body_bytes` | Fixed; `cargo test -p eqproxy-cli --test cli_e2e grpc_requests_obey_max_body_bytes_and_the_wasm_plugin` (red as `HTTP/2 502`, green as `413`/`403` with `grpc-status: 8`/`7`) |
| EXT-H-03 / P0-12 | PROXY preface is written after backend TLS | Fixed; PROXY line written on the raw stream before the handshake, unit + `cli_e2e` regressions red→green |
| EXT-H-04 / P0-13 | nginx `proxy_pass https://` imports as plaintext HTTP | Fixed; converted to TLS to the backend, `eqproxy-config/tests/import_corpus.rs` + real-binary E2E |
| EXT-H-05 / P0-14 | Backend TLS verifies the IP, not the configured hostname | Fixed; verified against `BackendConfig::declared_host`, `run_verifies_the_declared_backend_hostname_not_its_ip` |
| EXT-H-06 / P0-15 | `maxconn` is process-wide, not per listener | Fixed; per-listener counters with the public gauge kept as the sum, TCP and QUIC journeys through the real binary |

The details, the exact red→green commands and the remaining caveats are in
[release-0.10.0](release-0.10.0.md).

The one defect found **after** the tag is the `convert_duration` overflow of
P0-7 above: a panic under overflow checks and a wrong duration in the released
build. It is fixed on `main` (`513f6c9`) and **not** in `v0.10.0`, whose assets
were built from `bdf88bb`.

The EXT-M groups are still open: cache/HIT header duplication and h1
hop-by-hop stripping (P1-5), SNI/Host and duplicate/non-UTF-8 header semantics
(P1-6), WASM blocking and global h2/h3 stream budgets (P1-7), rate-limiter,
cache-assembly and header limits (P1-8), the rest of the per-server importer
semantics after `EXT-M-08`, `EXT-M-09` and `EXT-M-15` (P1-9), and
supply-chain/CI evidence (P1-10).

### Audit and platform limits

- **Veyra Proof (2026-11-16, v0.10.0 tree): exit 0, decision ACCEPTED, 566.83 s
  in tools, evidence `.veyra/evidence/20261116T201501Z/`.** 13 tools OK,
  including `cargo-semver-checks` (`cargo semver-checks check-release
  --baseline-rev v0.9.1 --release-type major` → `no semver update required` on
  all three crates) and `dylint`. `cargo-geiger` stays INCONCLUSIVE (coverage
  warnings in 4 runs, pre-existing). The previous run on the same tree before
  the version step (`.veyra/evidence/20261116T183815Z/`) was REJECTED on
  `semver-checks:semver-violation` and is kept on disk.
- **Rust Audit (2026-11-16):** `.github/scripts/rust-audit-gate.sh` exit 0 —
  6 reviewed high findings, 0 new, 0 extra, 0 stale.
- **CodeGraph (2026-11-16, rebuilt over this tree):** `codegraph-rust index .`
  reported `graph_complete: true`, `vector_index_ready: true`, 99 files,
  100 613 lines, 11 031 nodes, 22 540 edges (resolution 64.5 %), 883 496 ms and
  `errors: 0` (`/tmp/cg-0100.json`). `agent impact` and `agent context` ran on
  the touched slice, and every `file:line` was re-resolved with `rg` before it
  was used here:
  - `MapKey::select` — `eqproxy-core/src/maps.rs:481`, inside `impl MapKey`
    (`maps.rs:459`); confirmed. The graph records **no call edges into it**, so
    the production callers come from source: `rg -n '\.select\('
    eqproxy-core/src/rules.rs` returns `rules.rs:641` inside `Rule::matches`
    (fn at `rules.rs:606`) and `rules.rs:757` inside `Rule::explain` (fn at
    `rules.rs:667`). That closes the graph's own "explain not confirmed" gap
    with source, not with the graph.
  - Confirmed as cited: `host_key` `rules.rs:841`, the `map_or_else` header arm
    `maps.rs:488-492`, the test `maps.rs:620` with its six assertions
    `maps.rs:633-638`, the re-export `lib.rs:17`, `MapData::lookup`
    `maps.rs:247`, `HashPolicy::key_for` `proxy.rs:573`.
  - `ListenerConfig`: the graph holds the struct node but never returned its
    definition line. `rg` gives `pub struct ListenerConfig`
    `eqproxy-config/src/schema.rs:1135` and `pub redirect`
    `schema.rs:1149`, with consumers `validate.rs:721` and
    `main.rs:599/601/937/1999/3086`; the graph surfaced only the test-module
    import (`validate.rs:2910`) and `two_server_listener` (`main.rs:3844`).
  - nginx importer: `nginx::convert` `nginx.rs:835`, the dispatcher
    `import/mod.rs:1008` (wrappers at `973` and `992`), `Conv` at
    `mod.rs:462`, the CLI `fn import` `main.rs:1237` and the corpus tests
    `import_corpus.rs:312/480` are confirmed. Two graph lines did not survive
    the check: the include-variables note is at `nginx.rs:434`, not `433`, and
    its `MapData` import claim points at `eqproxy-net/src/maps.rs:14`, which
    imports `maps::{self, HotMap}` — the real `MapData` import is in test scope
    at line `128`.
  CodeGraph answers were used as leads only; nothing above rests on a graph
  sentence that `rg` did not reproduce.
- **HTTP/3** remains experimental; browser and `curl` h3 journeys and QUIC
  adversarial tests are not executed here.
- **macOS** is the local build platform. The release image and both Linux
  architectures were exercised by the release workflow's container journeys and
  read back here with `--version`; the native hosts are P1-1, not executed.
- **No users outside the author.** v0.10.0 has not been operated in production
  and does not claim production readiness.

## Conditions required before a production declaration

Done for `0.10.0`: fix and E2E-verify EXT-H-01…06; execute the final
Veyra/Rust Audit/CodeGraph legs on the final tree; run the fuzz workflow
remotely and cite its artifacts (`P0-7` found a real crash, fixed on `main` in
`513f6c9` and green in run `44324110633`); pass Docker Scout with zero
critical/high on both
architectures; publish and verify the image digest and checksums; re-run the
complete suite on the tag target.

Still required: cut the patch that carries `513f6c9` (the `convert_duration`
overflow is fixed on `main` but absent from the published `v0.10.0`); close the
EXT-M groups; run native `linux/amd64` and `linux/arm64` binary and image
journeys on the named hosts; and have someone other than the author operate the
published release. Until every one of those is executed, the verdict remains
**NOT PRODUCTION READY**.

## Evidence index

- [P0-8/P0-9 close-out evidence](evidence/v095-p0-8-p0-9.md)
- [P0-3/P0-4 and adversarial lab](evidence/v095-p0-3-p0-4.md)
- [P0-5 trailer budget and RSS](evidence/v095-p0-5.md)
- [P0-7/P1 fuzz, CI and audit record](evidence/v095-p1.md)
- [EXT-H reproduction](evidence/v095-ext-h-repro.md)
- [Current release preparation and publication status](release-0.10.0.md)
- [Historical v0.9.5 preparation record](release-0.9.5.md)
