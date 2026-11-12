# v0.9.5 — release record

Language: **English**.

**What this page answers:** what the `v0.9.5` tag contains, which gates ran on
that tagged tree, what was published on 2026-11-12 and with which evidence, and
what remains outside the tag.

- Version: **0.9.5**, from `[workspace.package] version` in the tagged
  `Cargo.toml`.
- Tag: annotated object `fd65bee0b62055e427afa4360560fca76d400813` over commit
  `3451c190362147a95e44eb1f08f306a39ec73125` (`fix(release): refresh Wolfi base
  for v0.9.5`), message `EQProxy v0.9.5`. That commit is
  `a76f017c099c8cf1e92b209a7d4e74cd8bc19220` (`release: prepare v0.9.5`) plus a
  one-line `docker/Dockerfile` change (`git diff --stat a76f017 v0.9.5` reports
  1 file, 1 insertion, 1 deletion). The tag is on the remote: `git ls-remote
  --tags origin refs/tags/v0.9.5` returns `fd65bee…` (checked 2026-11-12).
- Changelog: [`## [v0.9.5] — 2026-11-11`](../CHANGELOG.md).
- Tagged-tree gates: [v0.9.5 release evidence](evidence/v095-release.md) records
  `cargo fmt --all -- --check`, clippy, the workspace suite (799 passed, 0
  failed, 2 ignored), the release build, `git diff --check` and the fuzz
  workspace check, all exit 0.

## Publication — 2026-11-12

The GitHub release
[v0.9.5](https://github.com/EQProxy/EQProxy/releases/tag/v0.9.5) was published
on 2026-11-12T17:08:38Z by the release workflow, run
[43232479817](https://github.com/EQProxy/EQProxy/actions/runs/43232479817)
(`success`, tag push). A first run on the previous tag target
(`43097461215`, 2026-11-11) failed; the re-run on the retagged commit above
succeeded. Assets:

- `eqproxy-linux-amd64` and `eqproxy-linux-arm64`, static Linux binaries;
- `eqproxy-0.9.5-src.tar.gz`;
- one CycloneDX SBOM per crate (`eqproxy-core`, `eqproxy-config`, `eqproxy-net`,
  `eqproxy-cli`);
- `SCOUT-0.9.5-linux-amd64.md` and `SCOUT-0.9.5-linux-arm64.md`, each reporting
  **0 critical / 0 high** (per-image digests `sha256:3325290…` amd64 and
  `sha256:fcde920…` arm64);
- `SHA256SUMS.txt`, which in this release lists the two binaries and the
  source tarball only; the SBOMs and the two Scout reports are attached
  without a checksum line (the pipeline covers every asset from the next
  release).

Checks executed from this checkout on 2026-11-12 against the published
artifacts:

- Both binaries downloaded from the release and verified with
  `shasum -a 256 -c SHA256SUMS.txt --ignore-missing` → 2/2 `OK`.
- `file` reports `static-pie linked` (amd64) and `statically linked` (arm64),
  which is what the README claims about the published binaries.
- The registry APIs answer one multi-architecture index,
  `sha256:ec3413b8a2d871ae810e6cd81f6fbda679fc352a4b3659914a1977ecf5440bc0`,
  for both `:0.9.5` and `:latest` on `ghcr.io/eqproxy/eqproxy` and on
  `exyonq/eqproxy`, so the two namespaces serve identical digests.

**NOT EXECUTED here:** `docker run` of the published image or a re-pull, the
native `linux/amd64` and `linux/arm64` journeys on `netcup-bench` and
`oracle-quasar`, and CLI/traffic journeys of the published binaries (they are
Linux ELF executables and this host is macOS). The release workflow's own gate
jobs are cited by their run, not re-run locally.

## Post-tag P0-7/P1 work

A post-tag commit on `release/v0.9.5` adds the P0-7 fuzz/CI increment and the P1
cache/documentation close-out: `.github/workflows/fuzz.yml`, the report newline
fix found by `config_import`, the shared-cache authentication challenge
contract, and the P1 documentation review. Those changes are **not** in the
`v0.9.5` tag — the published tag carries `a76f017` plus the Docker base refresh
only — and no tag move for them was made. Evidence:
[v0.9.5 P0-7/P1](evidence/v095-p1.md).

The post-tag commit passed `cargo fmt --all -- --check`, clippy, the full
workspace suite (**804 passed, 0 failed, 2 ignored**), the fuzz workspace check,
`cargo +nightly fuzz build`, the release build and `git diff --check`; the
seeded fuzz campaigns and their counts are in the evidence page.

A later local commit closes P0-8/P0-9 (HAProxy header semantics, production
verdict and the move of the lab harness out of `docs/`). It is not in the
`v0.9.5` tag and no release was cut from it; evidence:
[P0-8/P0-9](evidence/v095-p0-8-p0-9.md).

The independent Veyra run on that post-tag tree is **ACCEPTED** (exit 0,
`.veyra/evidence/20261111T182801Z/`): the former `Metrics` patch-semver finding
is fixed by moving the counter into `DetailMetrics`; no version bump was needed.
`cargo test`/`nextest` and `cargo-semver-checks` pass (223/223 checks), and no
tool reports a blocking error. `cargo-geiger` is **INCONCLUSIVE** because some
dependency files were not scanned; it does not mention the removed temporary
test or stale dep-info. Details are in the P0-7/P1 evidence page.
