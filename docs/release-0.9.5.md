# v0.9.5 — release preparation record

Language: **English**.

**What this page answers:** what the `v0.9.5` tag contains, which gates ran on
that tagged tree, and what remains unpublished. This page does not move the tag
or claim a publication.

- Version: **0.9.5**, from `[workspace.package] version` in `Cargo.toml`.
- Tag: annotated object `002a7a10de2680b487c7ab3d65f0af714f8bae9a` over commit
  `a76f017c099c8cf1e92b209a7d4e74cd8bc19220` (`release: prepare v0.9.5`), message
  `EQProxy v0.9.5`. The tag is local: `git ls-remote --tags origin
  refs/tags/v0.9.5` returned no row on 2026-11-11.
- Changelog: [`## [v0.9.5] — 2026-11-11`](../CHANGELOG.md).
- Tagged-tree gates: [v0.9.5 release evidence](evidence/v095-release.md) records
  `cargo fmt --all -- --check`, clippy, the workspace suite (799 passed, 0
  failed, 2 ignored), the release build, `git diff --check` and the fuzz
  workspace check, all exit 0.

## Post-tag P0-7/P1 work

A post-tag commit on `release/v0.9.5` adds the P0-7 fuzz/CI increment and the P1
cache/documentation close-out: `.github/workflows/fuzz.yml`, the report newline
fix found by `config_import`, the shared-cache authentication challenge
contract, and the P1 documentation review. Those changes are **not** in the
`v0.9.5` tag; the commit is local and no tag move, push or workflow dispatch was
made. Evidence: [v0.9.5 P0-7/P1](evidence/v095-p1.md).

The post-tag commit passed `cargo fmt --all -- --check`, clippy, the full
workspace suite (**804 passed, 0 failed, 2 ignored**), the fuzz workspace check,
`cargo +nightly fuzz build`, the release build and `git diff --check`; the
seeded fuzz campaigns and their counts are in the evidence page.

A later local commit closes P0-8/P0-9 (HAProxy header semantics, production
verdict and the move of the lab harness out of `docs/`). It is not in the
`v0.9.5` tag and has no remote counterpart; evidence:
[P0-8/P0-9](evidence/v095-p0-8-p0-9.md).

The independent Veyra run on that post-tag tree is **ACCEPTED** (exit 0,
`.veyra/evidence/20261111T182801Z/`): the former `Metrics` patch-semver finding
is fixed by moving the counter into `DetailMetrics`; no version bump was needed.
`cargo test`/`nextest` and `cargo-semver-checks` pass (223/223 checks), and no
tool reports a blocking error. `cargo-geiger` is **INCONCLUSIVE** because some
dependency files were not scanned; it does not mention the removed temporary
test or stale dep-info. Details are in the P0-7/P1 evidence page.

## Not published

Publication remains **NOT EXECUTED**: no push, remote tag, GitHub release,
workflow dispatch, Docker Scout verdict, cross-architecture image E2E or
re-pull of a published image was performed. A green local tag is not a
published release.