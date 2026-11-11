# v0.9.6 — release preparation record

Language: **English**.

**What this page answers:** what the local `v0.9.6` tag contains, why it is
0.9.6 rather than 0.9.5, which gates ran on this tree, and what remains
unpublished. This page does not move an existing tag or claim a publication.

- Version: **0.9.6**, from `[workspace.package] version` in `Cargo.toml`.
- Tag: annotated `v0.9.6`, local only, pointing at the release commit that
  contains this file. The pre-existing annotated `v0.9.5` tag remains at
  `a76f017c099c8cf1e92b209a7d4e74cd8bc19220` and was neither moved nor
  deleted.
- Changelog: [`## [v0.9.6] — 2026-11-11`](../CHANGELOG.md).
- Tagged tree: `c9234d4` (P0-8/P0-9) plus the release-metadata commit that
  carries the version, changelog and this record.

## Why 0.9.6

The repository already had `Cargo.toml` at **0.9.5**, a `## [v0.9.5]` section
and an annotated `v0.9.5` tag at `a76f017`. That tag does not contain P0-8 and
is an ancestor of the current tree. It cannot be moved or deleted, so 0.9.5 is
not available for the current commit; **0.9.6** is the next patch version.

On 2026-11-11, `git ls-remote --tags origin refs/tags/v0.9.5
refs/tags/v0.9.6` returned no rows and `gh release view v0.9.5` answered
`release not found`. The pre-existing tag is local only.

## What this release contains

- **M-01:** HTTP/1.0 chunked framing remains fixed; the unit test and both
  E2E tests were executed red without the two HTTP/1.0 halves (exit 101) and
  green after restoring them. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).
- **P0-8:** HAProxy `hdr(<name>)` and `hdr_dom(<name>)` preserve distinct
  header and port-free Host semantics, with parser, conversion and real-binary
  E2E coverage. Evidence:
  [v095-p0-8-p0-9](evidence/v095-p0-8-p0-9.md).

The tag also includes the post-0.9.5 P0-7/P1 changes described in the
changelog; the fuzz workflow has not run in GitHub Actions, so P0-7 remains
open.

## Local gates

| Command | Result |
|---|---|
| `cargo test --workspace --locked` | exit 0 — **807 passed, 0 failed, 2 ignored** across 39 result blocks |
| Release workflow version pin (`test "v$VER" = "$GITHUB_REF_NAME"`) | exit 0 for `v0.9.6` |
| Release workflow changelog extraction | finds `## [v0.9.6]` and emits the section |
| `git diff --check` | exit 0 |

**Missing for publication:** the local tree is version/changelog/tag coherent,
but there is no push, remote tag, GitHub release, image, Scout verdict,
cross-architecture E2E or re-pull. P0-7 and the open P0/P1 work also remain
unclosed; the verdict is **NOT PRODUCTION READY**.
