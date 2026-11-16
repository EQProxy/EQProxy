# Production verdict — EQProxy v0.10.0

**Verdict: NOT PRODUCTION READY.**

This page is the production verdict for the v0.10.0 hardening tree. It separates
what has actually been executed from what is still a hypothesis, a local-only
result or a known open defect. A green local build is not a production
declaration.

## Revision and artifact

| Item | State |
|---|---|
| Branch | `release/v0.9.5` |
| Source revision | `da407ec` on `release/v0.9.5`, plus the uncommitted 0.10.0 release preparation (version bump, `veyra.toml`, release notes, ADR-0052); `v0.9.5` was published from `3451c19` |
| Version | `0.10.0` in `Cargo.toml`; `CHANGELOG.md` has `## [v0.10.0]` |
| Tag | **none for `0.10.0`**, local or remote; the earlier local annotated `v0.9.6` (`0f03bf4`) points at a tree whose `Cargo.toml` says `0.9.6`, so the workflow's version pin rejects it for this release; `v0.9.5` is annotated `fd65bee` → `3451c19`, remote and published 2026-11-12 |
| Release claims | only **M-01** and **P0-8**; P0-7, P0-10…P0-15 and P1-1…P1-10 remain open |
| Published artifact | **none for 0.10.0**: the branch is on origin (`release/v0.9.5` at `ca1f0a9`) with this tree ahead of it and unpushed; no `v0.10.0` tag, GitHub release, registry image, digest promotion or re-pull; the published release is `v0.9.5` (2026-11-12) |
| Final local suite | `cargo test --workspace` → **873 passed, 0 failed, 2 ignored** (41 result blocks) on 2026-11-16; fmt, clippy `-D warnings` and `cargo dylint --all` exit 0; see [release-0.10.0](release-0.10.0.md) |

## What the evidence actually shows

- **Workspace suite:** `cargo test --workspace` on this tree (`da407ec` plus the
  0.10.0 preparation) ran **873 passed, 0 failed, 2 ignored** across **41 result
  blocks** on 2026-11-16, inside the Veyra run cited above
  (`.veyra/evidence/20261116T201501Z/`). The earlier P0-8/P0-9 tree (`0f03bf4`)
  recorded 807 passed in 39 blocks; the difference is the v0.9.6 import work —
  importer parser and conversion, the corpus fixture move and the real-binary
  E2E — plus the HTTP/3 hot-reload E2E. Historical detail:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
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

- **P0-7 / remote CI:** `.github/workflows/fuzz.yml` exists locally, but the
  workflow has **not run in GitHub Actions**. There is no remote run, artifact
  or log; this is **NOT EXECUTED**, not a pass.
- **P1-1 / architectures:** no native `linux/amd64` journey on `netcup-bench`
  and no native `linux/arm64` journey on `oracle-quasar` for this tree.
- **Docker / Scout:** no image build, no native image E2E and no Docker Scout
  critical/high gate for v0.10.0. **NOT EXECUTED.**
- **Publication:** no remote `v0.10.0` tag, GitHub release, image digest or
  checksum of an artifact built from this tree exists. `v0.9.5` was published
  from a different commit (`3451c19`), so it does not cover this tree.
- **P1-4:** version and changelog exist at `0.10.0`, but the release is not
  closed; there is no tag and a tag is not a publication.

### Open defects

The six external high-severity findings are **REPRODUCED, not fixed** on this
tree:

| ID | Defect | State |
|---|---|---|
| EXT-H-01 / P0-10 | Compressed cache HIT mixes compressed headers with an identity body | REPRODUCED; no fix/E2E |
| EXT-H-02 / P0-11 | gRPC bypasses the WASM plugin and `max_body_bytes` | REPRODUCED; no fix/E2E |
| EXT-H-03 / P0-12 | PROXY preface is written after backend TLS | REPRODUCED; no fix/E2E |
| EXT-H-04 / P0-13 | nginx `proxy_pass https://` imports as plaintext HTTP | REPRODUCED; no fix/E2E |
| EXT-H-05 / P0-14 | Backend TLS verifies the IP, not the configured hostname | REPRODUCED; no fix/E2E |
| EXT-H-06 / P0-15 | `maxconn` is process-wide, not per listener | REPRODUCED; no fix/E2E |

The EXT-M findings are still open: cache/HIT header duplication and h1
hop-by-hop stripping (P1-5), SNI/Host and duplicate/non-UTF-8 header semantics
(P1-6), WASM blocking and global h2/h3 stream budgets (P1-7), rate-limiter,
cache-assembly and header limits (P1-8), per-server importer/secret/atomic-write
semantics (P1-9), and supply-chain/CI evidence (P1-10).

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
- **CodeGraph** was not rebuilt over the v0.10.0 tree; a query against a stale
  index is **NOT EXECUTED**, never a pass.
- **HTTP/3** remains experimental; browser and `curl` h3 journeys and QUIC
  adversarial tests are not executed here.
- **macOS** is the local build platform; the release image and both Linux
  architectures are unverified for this tree.
- **No users outside the author.** v0.10.0 has not been operated in production
  and does not claim production readiness.

## Conditions required before a production declaration

At minimum: fix and E2E-verify EXT-H-01…06; close the EXT-M groups; execute the
final Veyra/Rust Audit/CodeGraph legs on the final tree; run the fuzz workflow
remotely and cite its artifacts; run native amd64 and arm64 binary/image
journeys; pass Docker Scout with zero critical/high; publish and verify the
image digest and checksums; then re-run the complete suite on the published
artifact. Until every one of those is executed, the verdict remains **NOT
PRODUCTION READY**.

## Evidence index

- [P0-8/P0-9 close-out evidence](evidence/v095-p0-8-p0-9.md)
- [P0-3/P0-4 and adversarial lab](evidence/v095-p0-3-p0-4.md)
- [P0-5 trailer budget and RSS](evidence/v095-p0-5.md)
- [P0-7/P1 fuzz, CI and audit record](evidence/v095-p1.md)
- [EXT-H reproduction](evidence/v095-ext-h-repro.md)
- [Current release preparation and publication status](release-0.10.0.md)
- [Historical v0.9.5 preparation record](release-0.9.5.md)
