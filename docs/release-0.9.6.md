# v0.9.6 — release preparation record

Language: **English**.

**What this page answers:** what the local `v0.9.6` tag points at, which
release claims are actually verified, why the tag was not realigned after the
documentation correction, and what remains unpublished. This page does not
move an existing tag or claim a publication.

- Version: **0.9.6**, from `[workspace.package] version` in `Cargo.toml`.
- Tag: annotated `v0.9.6`, local only, pointing at release commit
  `0f03bf4864a0557f1e23f280be052fe9a7b6bd1a`. The tag was not moved.
- Pre-existing tag: annotated `v0.9.5` remains at
  `a76f017c099c8cf1e92b209a7d4e74cd8bc19220`; it was neither moved nor
  deleted.
- Current tree: `0f03bf4` plus a later **documentation correction commit**
  (HEAD) that corrects the release claims. That correction is not in the
  `v0.9.6` tag.

## Why 0.9.6

The repository already had `Cargo.toml` at **0.9.5**, a `## [v0.9.5]` section
and an annotated `v0.9.5` tag at `a76f017`. That tag does not contain P0-8 and
is an ancestor of the current tree. It cannot be moved or deleted, so 0.9.5 is
not available for the current commit; **0.9.6** is the next patch version.

On 2026-11-11, `git ls-remote --tags origin refs/tags/v0.9.5
refs/tags/v0.9.6` returned no rows and `gh release view v0.9.5` answered
`release not found`. That observation no longer holds: on 2026-11-12 `v0.9.5`
was published (tag `fd65bee…` → `3451c19`, release run `43232479817`), see
[the v0.9.5 release record](release-0.9.5.md). **`v0.9.6` remains unpublished**
on every count.

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

The tag target also contains the post-0.9.5 local changes from `a0ac3fc`, but
they are **not** release-cleared here: P0-7 has no GitHub Actions run, and
P0-10…P0-15 plus P1-1…P1-10 remain open. No P0/P1 completion is claimed.

## Tag target and correction

The annotated `v0.9.6` tag remains at **`0f03bf4`**. The later correction
commit fixes only release documentation; it does not change the tagged tree.
Publishing from the tag would therefore use the older release text, which
still listed P0-7 under `Added`. Moving the tag to the correction commit is a
tag move, not authorized here; it requires an owner decision. The current
working tree is the corrected one.

## Local gates

| Command | Result |
|---|---|
| `cargo test --workspace --locked` | exit 0 — **807 passed, 0 failed, 2 ignored** across 39 result blocks |
| Release workflow version pin (`test "v$VER" = "$GITHUB_REF_NAME"`) | exit 0 for `v0.9.6` on the tag target and on current HEAD |
| Release workflow changelog extraction on current HEAD | finds `## [v0.9.6]` and emits the corrected section |
| `git diff --check` | exit 0 |

**Missing for publication of 0.9.6:** the branch `release/v0.9.5` is on origin
(`84e5eb3`), but there is no remote `v0.9.6` tag, no GitHub release for it, no
Docker Scout gate, no cross-architecture amd64/arm64 E2E and no re-pull of an
image built from this tree; the published release is still `v0.9.5`. P0-7 and
the open P0/P1 work remain unclosed. The tag/version pair is consistent, but the tag target still
contains the pre-correction release text; realigning it requires a tag move
that was not performed. The verdict is **NOT PRODUCTION READY**.
