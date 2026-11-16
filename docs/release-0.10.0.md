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
- Current tree: `da407ec` on `release/v0.9.5` plus this release preparation,
  which is not committed yet.
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

Only **M-01** and **P0-8** are claimed as closed fixes:

- **M-01:** HTTP/1.0 chunked framing remains fixed; the unit test and both
  E2E tests were executed red without the two HTTP/1.0 halves (exit 101) and
  green after restoring them. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
- **P0-8:** HAProxy `hdr(<name>)` and `hdr_dom(<name>)` preserve distinct
  header and port-free Host semantics, with parser, conversion and real-binary
  E2E coverage. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).

P0-7 has no GitHub Actions run; P0-10…P0-15 and P1-1…P1-10 remain open. No
P0/P1 completion is claimed, and the version step is an API-compatibility
decision, not a feature claim.

## Local gates on this tree (2026-11-16)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | exit 0 — **873 passed, 0 failed, 2 ignored** across 41 result blocks |
| `cargo dylint --all` (`nightly-2026-05-28`) | exit 0 |
| `cargo semver-checks check-release --baseline-rev v0.9.1 --release-type major` | exit 0 — `eqproxy-config`, `eqproxy-core` and `eqproxy-net` each report `no semver update required` |
| Release workflow version pin (`test "v$VER" = "$GITHUB_REF_NAME"`) | `VER=0.10.0`, exit 0 for a `v0.10.0` tag |
| Release workflow changelog extraction | finds `## [v0.10.0] — 2026-11-16` in `CHANGELOG.md` |
| Rust Audit gate (`.github/scripts/rust-audit-gate.sh`) | exit 0 — 6 reviewed high findings, 0 new, 0 extra, 0 stale |
| `veyra audit . --profile standard` | exit 0 — **ACCEPTED**, 13 tools OK (`rustfmt`, `cargo check`, `clippy`, `cargo test`, `cargo-audit`, `veyra-anti-theater`, `nextest`, `llvm-cov`, `cargo-deny`, `cargo-machete`, `dylint`, **`cargo-semver-checks`**, `veyra-e2e`); `cargo-geiger` INCONCLUSIVE as before; 566.83 s in tools; evidence `.veyra/evidence/20261116T201501Z/` |

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
