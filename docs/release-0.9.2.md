# v0.9.2 — publication record (Linux-only prebuilt binaries)

Language: **English**.

**What this page answers:** what `v0.9.2` published, from which commit and run,
with which assets and which image digests, and what the local preparation
before the tag proved or failed to prove. The first four sections are
publication facts verified against GitHub and the two registries; section 6
keeps the local preparation builds for traceability and is explicitly **not**
what the release ships.

- Version: **0.9.2** (`Cargo.toml` `[workspace.package] version`).
- Branch: `harden/config-gate-and-errors`; the tag target is `6478f10`
  (`release: publish Linux-only v0.9.2 artifacts`).
- Tag: annotated object `sha1:1ba58ea21388a5a69f8a3d577576ae9e14ec312b` over
  commit `6478f10b2d92792f9dba1c0cec5753fb738d9e89`, message `EQProxy v0.9.2`.
  The remote carries the same tag object, and the branch now reaches the same
  commit, so the tagged commit is in remote branch history as well.
- Release: [v0.9.2](https://github.com/EQProxy/EQProxy/releases/tag/v0.9.2),
  published `2026-11-06T23:11:38Z`, not a draft, not a pre-release.
- Pipeline run: [42281241472](https://github.com/EQProxy/EQProxy/actions/runs/42281241472)
  (`Release`, `push`, branch `v0.9.2`, head `6478f10`) — `success`.
- Release notes: [CHANGELOG.md](../CHANGELOG.md), section
  `## [v0.9.2] — 2026-11-06` (68 extracted lines).
- Pipeline: [.github/workflows/release.yml](../.github/workflows/release.yml).

`README.md` is version-agnostic: install commands use the moving `latest` tag
and `releases/latest`, so it needs no per-release edit.

## 1. Version and manifests

| File | Change |
|---|---|
| `Cargo.toml` | workspace version `0.9.2` |
| `eqproxy-cli/Cargo.toml` | internal pins `eqproxy-core`, `eqproxy-config`, `eqproxy-net` at `0.9.2` |
| `eqproxy-config/Cargo.toml` | `eqproxy-core` pin at `0.9.2` |
| `eqproxy-net/Cargo.toml` | `eqproxy-core` pin at `0.9.2` |
| `Cargo.lock`, `fuzz/Cargo.lock` | all four local packages at `0.9.2` |
| `CHANGELOG.md` | dated `v0.9.2` section |
| `docker/Dockerfile` | Wolfi base pinned to `sha256:8509ee4351a2e501b7f98201f2e2642396e82020c61a589cacdcfe74b9de46c1`; example tag `0.9.2` |

A stale `0.9.1` dependency pin was found in the three member manifests and
fixed before the final Linux build. The workspace and both lockfiles were
regenerated with `cargo metadata`; `cargo metadata --locked` resolves the four
crates at `0.9.2`.

## 2. The publication run

Run [42281241472](https://github.com/EQProxy/EQProxy/actions/runs/42281241472)
started `2026-11-06T19:37:18Z` and finished `2026-11-06T23:12:03Z` on
`6478f10`, conclusion `success`. Its five jobs:

| Job | Conclusion | Window (UTC) |
|---|---|---|
| Line gates pass before any build | `success` | 19:37:22 → 19:44:57 |
| Binaries linux (build + E2E verify) + SBOM | `success` | 19:45:01 → 21:29:32 |
| Multi-arch image (build + Scout both archs + E2E + promote) | `success` | 21:29:35 → 23:11:25 |
| GitHub release (binaries + source tarball + SHA256SUMS + SBOM + Scout) | `success` | 23:11:28 → 23:11:39 |
| Docker Hub page (rendered, published and read back) | `success` | 23:11:42 → 23:12:02 |

The image job built `docker/Dockerfile` with
`docker buildx build --pull --platform linux/<arch>` for both architectures,
so the earlier local limitation in section 6 (the full multi-stage build never
completing on this Mac) is closed by CI and not by a local rerun. The source
tarball was exported from the tagged commit by the `release` job, not from a
working tree. The container journeys and the guard suite are the run log's
transcript, not this page's.

The tag was pushed more than once while the cut was being fixed. Two earlier
tag runs did not produce the release: [42259220860](https://github.com/EQProxy/EQProxy/actions/runs/42259220860)
(`failure`, head `b97b71a`) and [42261566878](https://github.com/EQProxy/EQProxy/actions/runs/42261566878)
(`cancelled`, head `ba3824c`). The published assets and images belong to the
successful run above, whose head is the tag target.

## 3. Published assets

Ten assets, all `uploaded`. Downloaded with `curl` from
`releases/download/v0.9.2/` and checked with
`shasum -a 256 -c SHA256SUMS.txt`: **3/3 OK** (the two binaries and the source
tarball; the SBOMs and Scout reports are not listed in `SHA256SUMS.txt`). The
`SHA-256` column is GitHub's own digest of each uploaded asset and matches the
downloaded bytes.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `eqproxy-linux-amd64` | 37 253 328 | `e12aa71290ab6a9c9b69f585f3f765a445e2130790bfa72149668c0a45b59929` |
| `eqproxy-linux-arm64` | 31 705 544 | `7946fa089b6d8c74590763d913e46e9ad183002b55a61425eb87ec8b103bc29e` |
| `eqproxy-0.9.2-src.tar.gz` | 6 689 523 | `a466d37098e203be1819b142c68fb7e78bb9e955b0bc545920b7630f64bf7cdd` |
| `eqproxy-cli.cdx.json` | 237 916 | `76fa60a37085cab5c712b6fdfea9d09d3e495c41ad9b46a10ee55e680402567e` |
| `eqproxy-net.cdx.json` | 205 755 | `180c82ece007d3a3f9dfcb2d600fbf8cf048940b0e152513d03cd42ee8ee7f9c` |
| `eqproxy-config.cdx.json` | 59 576 | `d203b01d66e8dcce29d23fd0e47327a1048040628627ff3e0261f3c536eecbf5` |
| `eqproxy-core.cdx.json` | 36 560 | `76c265ac5a8ae7cef14e3642371a0781a56d9eff9efad0c5f447c69777ea9f97` |
| `SCOUT-0.9.2-linux-amd64.md` | 1 170 | `217766ef2bfd606dad436deaa0c6398b955d8f1e23be84cf3804bd0517bda01d` |
| `SCOUT-0.9.2-linux-arm64.md` | 1 170 | `280e5c52d366ec81e0a593576828b23fac339e5ee46d45cf5355b8a3118f21f6` |
| `SHA256SUMS.txt` | 263 | `998bf36883f7a31bb042608e38fd8b0ebdf958170b9f9c4807a42d4e559852ec` |

`SHA256SUMS.txt` lists exactly three lines — `eqproxy-linux-amd64`,
`eqproxy-linux-arm64` and `eqproxy-0.9.2-src.tar.gz` — with the digests above
in that order.

The two downloaded binaries are the expected shapes:

```console
$ file eqproxy-linux-amd64 eqproxy-linux-arm64
eqproxy-linux-amd64: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV),
  static-pie linked, BuildID[sha1]=d9cf6b382bf4b21711bf5074ced55b51acd048c8, not stripped
eqproxy-linux-arm64: ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV),
  statically linked, BuildID[sha1]=4c87187f2f6279d411fe57e0ed2959ca042844ae, not stripped
```

There is no Darwin asset: the release is Linux-only, and the workflow has no
Darwin job.

## 4. Published container images

One OCI index was built, scanned and then promoted by digest, so the release
tags and the mirror are the same bytes. `docker buildx imagetools inspect`
against all four published references returns one index digest:

| Reference | Index digest |
|---|---|
| `ghcr.io/exyonq/eqproxy:0.9.2` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
| `ghcr.io/exyonq/eqproxy:latest` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
| `docker.io/exyonq/eqproxy:0.9.2` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
| `docker.io/exyonq/eqproxy:latest` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |

That index — `application/vnd.oci.image.index.v1+json` — contains:

| Platform | Manifest digest |
|---|---|
| `linux/amd64` | `sha256:b984b522fa32b82830194183a52a8238f34a5dc95545a29aff8d574ede7084e6` |
| `linux/arm64` | `sha256:d8b681d54304202ce24149390a05140f61448cf5a68c32a56a358437525f29ba` |
| `unknown/unknown` (attestation for amd64) | `sha256:31bbabeee6494736c01e14773e373e30187f4f059c0113c8c85b0cc255b0f03f` |
| `unknown/unknown` (attestation for arm64) | `sha256:f9558d3fa1a582a39253e37a0f1b7e005e0b6f11fe7d3407597c7470e8d1e4ad` |

### Docker Scout gate

Both published reports name the candidate index
`sha256:6392aa30…ea3f`, the same digest that was promoted, and both rejected
nothing:

| | linux/amd64 | linux/arm64 |
|---|---|---|
| report asset | `SCOUT-0.9.2-linux-amd64.md` | `SCOUT-0.9.2-linux-arm64.md` |
| critical / high | **0 / 0** | **0 / 0** |
| medium / low | 0 / 0 | 0 / 0 |
| packages | 69 | 69 |
| size | 20 MB | 18 MB |

Re-checked on the published tag, not only on the candidate: with
`docker scout cves --only-severity critical,high --exit-code
ghcr.io/exyonq/eqproxy:0.9.2`, the local platform (`linux/arm64`) answered
**0 critical / 0 high / 0 medium / 0 low**, 69 packages, 18 MB, base
`cgr.dev/chainguard/wolfi-base:latest` @ `8509ee4351a2`, exit **0**, provenance
`https://github.com/EQProxy/EQProxy/blob/6478f10b2d92792f9dba1c0cec5753fb738d9e89`
— the tag target.

### Pulled and run

```console
$ docker run --rm ghcr.io/exyonq/eqproxy:0.9.2 --version
Digest: sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f
eqproxy 0.9.2
```

The pull resolved the index digest above and the runtime answered its own
version. The same `--version` through `docker.io/exyonq/eqproxy:0.9.2` also
exited 0. Runtime user, CLI journeys (`config validate`, `config apply` as uid
65532) and live proxied traffic were exercised inside the image by the image
job before promotion; that transcript is the run log.

## 5. Local gates before the tag

These were measured locally during preparation, on the cut tree, before the
tag existed. The tag run's own line-gate job repeated the gated suite on
`6478f10` and finished `success`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit **0** |
| `cargo test --workspace --locked` | exit **0** — **738 passed, 2 ignored, 0 failed** |
| macOS `eqproxy-darwin-arm64` E2E (local only, not shipped) | `--version`, validate, missing-file error, explain, diff, apply/rollback, doctor and live proxied traffic: **PASS** |
| macOS `eqproxy-darwin-amd64` E2E (local only, not shipped) | same journeys, including Rosetta live traffic: **PASS** (`MAC_E2E=PASS`, `MAC_FULL_CLI_EXIT=0`) |
| `check-evidence-test-citations.py`, `check-short-health-probes.py`, `check-secrets.py` | self-test and tree runs passed before this cut; the cut tree uses the same gate inputs |
| Release-note extraction | `## EQProxy v0.9.2 — 2026-11-06`, 68 lines |
| Toolchain invariant | `rust-toolchain.toml` and `ARG RUST_VERSION` both `1.99.1` |

The tests were run from the fixed manifest tree, not from an earlier tree with
the stale pins.

## 6. Local preparation builds — superseded, not shipped

`dist-v092/` is a local, untracked directory. It is **not** the release: the
Linux binaries there were compiled by hand in `rust:1.99.1-alpine` before the
tag, and their digests differ from the published assets in section 3, which
were built by the tag run from `6478f10`. The Darwin binaries were verified
locally and never published. Hashes are SHA-256.

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `eqproxy-darwin-arm64` | 29 324 224 | `28cea85e195a759d31a5257ec87237143108b37b65db1c39b418443effb660cd` |
| `eqproxy-darwin-amd64` | 34 105 008 | `afee2d32410a48e8ad85f078c8b26ab0dd84694e3966860378a2f0adde9b8829` |
| `eqproxy-linux-arm64` | 31 706 640 | `9ecf64c93ca7d48c2860d6d137f3d64fee60c0ec763f520bea841842fff90cc8` |
| `eqproxy-linux-amd64` | 37 241 632 | `59fd4923eaa04afc20aad8515d50f53b1e51268478b9e53251ad742646c3686d` |

`dist-v092/SHA256SUMS.txt` contains all four entries and
`shasum -a 256 -c SHA256SUMS.txt` reported `4/4 OK` there.

The Linux builds used `CARGO_BUILD_JOBS=1` because the host's system-wide file
table was exhausted under BuildKit and the normal multi-job build (`Too many
open files in system`). Both exited 0 and produced static musl executables.

Two local container images (`eqproxy-release:0.9.2-arm64`, image ID
`sha256:f7b5263e…`; `eqproxy-release:0.9.2-amd64`, image ID
`sha256:d2f36706…`) were assembled from those hand-built binaries on top of
the pinned Wolfi base, scanned 0 critical / 0 high by Docker Scout and
exercised with live traffic to an `nginx:alpine` origin (`HTTP 200` on both).
They are preparation evidence; the published images are section 4's index,
built by CI from `docker/Dockerfile`. The full multi-stage Dockerfile never
completed on this Mac because BuildKit stalled at the `h3-quinn` dependency
stage with zero CPU and no error; that limitation is why the CI image job is
the build of record.

## 7. Audit legs

| Leg | Result |
|---|---|
| Veyra Proof (local preparation, `6478f10` tree) | **REJECTED** (exit 1) at `20261106T151720Z`. `rustfmt`, check, clippy, cargo test, nextest, audit, deny, machete, dylint and veyra-e2e passed. `cargo-llvm-cov` and `cargo-semver-checks` exited 101 with `Too many open files in system`; `cargo-geiger` was INCONCLUSIVE with 1,005 unscanned-file warnings. Evidence: `.veyra/evidence/20261106T151720Z/report.md` (local-only). Not rerun for this record. |
| Veyra Proof (this record, 2026-11-07) | **ACCEPTED**, exit **0**, `20261107T004348Z`, on a working tree that differs from `6478f10` only in this Markdown page. `rustfmt`, check, clippy, cargo test, nextest, llvm-cov, audit, deny, machete, dylint, semver-checks, veyra-e2e and anti-theater OK; nothing skipped, nothing missing. `cargo-geiger` stays INCONCLUSIVE (coverage warnings in 4 runs) and is not counted as a pass. Evidence: `.veyra/evidence/20261107T004348Z/report.md` (local-only). |
| Rust Audit (local preparation) | exit 1 by threshold: **0 critical / 6 high / 436 medium / 114 low**. All six high triaged: `pprof` is the real optional profiler dependency (not `approx`); both `build.rs` return before invoking `sh` when `PROFILE=release` (test certificates only in debug); the three `rustc_ast`/`rustc_errors`/`rustc_lint` imports are compiler-private crates used by the Dylint lint crate with `#![feature(rustc_private)]`. Report: `report/rust-audit-20261106-154958.md` (local-only). |
| CodeGraph Rust | **NOT EXECUTED**. On the cut the index failed with `Ollama embedding API error ... socket: too many open files in system` (`/tmp/eqproxy-codegraph.log`). Re-attempted on 2026-11-07 over the tree of this record: the graph phase completed and persisted 9,334 nodes, then the semantic embedding phase stopped emitting output at `00:53:37Z` and the process sat at 0.0 % CPU with an unchanged 8.1 GB store for 14 minutes; it was stopped at 13:38 elapsed and wrote no `--stats-json`. No query is cited against either index. |
| Hindsight (during the cut) | **NOT EXECUTED**: the local service returned `status:error` (`mlx runner failed: missing embedding weight: embed_tokens.weight`). |
| Hindsight (close-out of this record) | **Stored** in bank `eqproxy`, document `facd08e6-3790-4531-a048-a8a283a31ef1` (12 memories, literal verified, 3 117 B), exit **0**. The note carries the run id, the tag object, the asset and index digests of sections 3 and 4, and the branch push range. |

The Veyra and CodeGraph failures are environmental file-descriptor exhaustion
on the maintainer host, not code findings. The tag run's `Line gates pass
before any build` job is the CI evidence that the gated suite passed on
`6478f10`; it is not a Veyra verdict and does not replace one.

## 8. Pending

1. `CONTRIBUTING.md` names `v0.9.2` as the latest published release and carries
   this run, the three checksummed assets and the image digests. That edit lives
   in the local working tree only: it is not part of this commit, and the tag
   does not contain it either.
2. For the `v0.9.2` cut the local Veyra run was **REJECTED** (host
   file-descriptor exhaustion); the 2026-11-07 rerun over this tree is
   **ACCEPTED** (section 7). CodeGraph is **NOT EXECUTED** for both trees: no
   `--stats-json` was produced, so no structural claim is made and none may be
   described as green.
3. This page describes the release after the fact. The published tree at
   `6478f10` carries the preparation text of this same file; the publication
   facts live in the commit that adds this section, not inside the tag.
