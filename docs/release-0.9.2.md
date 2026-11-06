# v0.9.2 — release preparation (Linux-only prebuilt binaries)

Language: **English**.

**What this page answers:** what has been verified locally for the `v0.9.2`
cut, which artifacts exist, and which artifacts the Linux-only release ships.
It is a preparation record, **not** a publication record.

- Version: **0.9.2** (`Cargo.toml` `[workspace.package] version`).
- Branch: `harden/config-gate-and-errors` (the release commit is the
  `v0.9.2` tag target).
- Release notes: [CHANGELOG.md](../CHANGELOG.md), section
  `## [v0.9.2] — 2026-11-06` (68 extracted lines).
- Pipeline: [.github/workflows/release.yml](../.github/workflows/release.yml).
- State: **Linux-only release prepared; publication is owned by the tag
  pipeline.**

`README.md` is version-agnostic: install commands use the moving `latest`
tag and `releases/latest`, so it does not need a version edit per release.
`CONTRIBUTING.md` still names `v0.9.1` as the last published release and
`v0.9.2` as in preparation; update it only after the tag exists. Pointing
install URLs at a version that has not been published would be a broken link,
not a version bump.

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
regenerated with `cargo metadata`; `cargo metadata --locked` resolves the
four crates at `0.9.2`.

## 2. Local gates

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit **0** |
| `cargo test --workspace --locked` | exit **0** — **738 passed, 2 ignored, 0 failed** |
| macOS `eqproxy-darwin-arm64` E2E (local only, not shipped) | `--version`, validate, missing-file error, explain, diff, apply/rollback, doctor and live proxied traffic: **PASS** |
| macOS `eqproxy-darwin-amd64` E2E (local only, not shipped) | same journeys, including Rosetta live traffic: **PASS** (`MAC_E2E=PASS`, `MAC_FULL_CLI_EXIT=0`) |
| `check-evidence-test-citations.py`, `check-short-health-probes.py`, `check-secrets.py` | self-test and tree runs passed before this cut; the cut tree still uses the same gate inputs |
| Release-note extraction | `## EQProxy v0.9.2 — 2026-11-06`, 68 lines |
| Toolchain invariant | `rust-toolchain.toml` and `ARG RUST_VERSION` both `1.99.1` |

The tests were run from the fixed manifest tree, not from an earlier tree with
the stale pins.

## 3. Artifacts built locally

`dist-v092/` is a local, untracked directory. Hashes are SHA-256. Only the two
Linux binaries are release assets: the release workflow has no Darwin job, and
the macOS binaries below were verified locally during preparation only, never
published.

| Artifact | Size | SHA-256 | Verification |
|---|---:|---|---|
| `eqproxy-darwin-arm64` | 28 MB | `28cea85e195a759d31a5257ec87237143108b37b65db1c39b418443effb660cd` | Mach-O arm64; full CLI + live traffic |
| `eqproxy-darwin-amd64` | 33 MB | `afee2d32410a48e8ad85f078c8b26ab0dd84694e3966860378a2f0adde9b8829` | Mach-O x86_64; full CLI + live traffic |
| `eqproxy-linux-arm64` | 30 MB | `9ecf64c93ca7d48c2860d6d137f3d64fee60c0ec763f520bea841842fff90cc8` | ELF aarch64, statically linked; image E2E |
| `eqproxy-linux-amd64` | 36 MB | `59fd4923eaa04afc20aad8515d50f53b1e51268478b9e53251ad742646c3686d` | ELF x86-64 static-pie; image E2E |

`dist-v092/SHA256SUMS.txt` contains all four entries and
`shasum -a 256 -c SHA256SUMS.txt` reports `4/4 OK`.

The Linux binaries were built in `rust:1.99.1-alpine` with
`CARGO_BUILD_JOBS=1` because the host's system-wide file table was exhausted
under BuildKit and the normal multi-job build (`Too many open files in
system`). Both builds exited 0 and produced static musl executables; the
`amd64` one is a static-pie ELF and the `arm64` one is statically linked.


## 4. Container images

### Mandatory Docker Scout gate

The release workflow builds one candidate index, records its digest, scans
**both** `linux/amd64` and `linux/arm64` with
`docker scout cves --only-severity critical,high --exit-code`, and requires
both markdown reports before the `release` job can assemble assets. The
public tags are created by promoting that exact scanned digest; a failing
scan stops the job before any tag or release is promoted. This is a hard gate,
not a manual check: a missing report on a tag run fails the release.

### linux/arm64

- Image: `eqproxy-release:0.9.2-arm64`
- Image ID: `sha256:f7b5263ef781f7fb2d5160688e46a64e34e4c92c6300b478a35ec61fd4a5999d`
- Size: 68.2 MB; runtime filesystem 18 MB.
- Base: `cgr.dev/chainguard/wolfi-base:latest@sha256:8509ee4351a2e501b7f98201f2e2642396e82020c61a589cacdcfe74b9de46c1`
- `/etc/os-release`: `ID=wolfi`; `USER`: `65532:65532`; `id`: `uid=65532(nonroot)`.
- `/opt/eqproxy --version`: `eqproxy 0.9.2`.
- `config validate`: `config OK: 1 listener(s), 1 backend(s)`.
- `config apply` as uid 65532: exit **0**, wrote
  `/var/lib/eqproxy/current.toml`.
- Docker Scout:
  `docker scout cves --only-severity critical,high --exit-code --platform linux/arm64 eqproxy-release:0.9.2-arm64`
  → exit **0**, **0 critical / 0 high / 0 medium / 0 low**, 36 packages.
- Live traffic through the image to an `nginx:alpine` origin:
  `HTTP 200` (`arm64_image_e2e=PASS`).

### linux/amd64

- Image: `eqproxy-release:0.9.2-amd64`
- Image ID / digest: `sha256:d2f3670660591ed324b41d3026a9e75b8b9f6ebf81b9ea21f25ce81916e1f7f7`
- Architecture: `amd64/linux`; size 77.3 MB; runtime filesystem 20 MB.
- Base: `cgr.dev/chainguard/wolfi-base:latest@sha256:8509ee4351a2e501b7f98201f2e2642396e82020c61a589cacdcfe74b9de46c1`
- `/etc/os-release`: `ID=wolfi`, `VERSION_ID="20230201"`; `USER`:
  `65532:65532`; `id`: `uid=65532(nonroot)`.
- `/opt/eqproxy --version`: `eqproxy 0.9.2`; `--help` starts with the real
  CLI command list.
- `config validate --config /c.toml`: exit **0**, `config OK: 1 listener(s),
  1 backend(s)` (plus the expected cleartext-listener warning).
- `config apply --config /c.toml` as uid 65532: exit **0**, wrote
  `/var/lib/eqproxy/current.toml`; the dry-run plan listed the real listener
  and backend.
- Docker Scout:
  `docker scout cves --only-severity critical,high --exit-code --platform linux/amd64 eqproxy-release:0.9.2-amd64`
  → exit **0**, **0 critical / 0 high / 0 medium / 0 low**, 36 packages.
- Live traffic through the image to an `nginx:alpine` origin: **HTTP 200**,
  body `Welcome to nginx`, proxy container `running`
  (`AMD64_IMAGE_E2E=PASS`).

An earlier E2E attempt used a Docker service name for the backend and
returned `503`; it was discarded, not counted. The accepted run used a
static backend IP on an isolated `/24` and a random host port, and checked
the origin and proxy independently before the request.

### Build-method limitation

Docker BuildKit on this Mac repeatedly stalled at the same dependency stage
(`h3-quinn`) with zero CPU and no error; stopping and restarting it did not
make it a code failure, but it also did not produce an artifact. Both images
above use the exact runtime stage from `docker/Dockerfile` over the pinned
Wolfi digest, but their binary was compiled in a separate
`rust:1.99.1-alpine` container. The `amd64` runtime was then built with
BuildKit; the legacy builder failed while exporting the platform-specific
intermediate image, which is a builder limitation, not a binary failure. The
full multi-stage `docker/Dockerfile` itself has **not** completed locally; the
CI image job must build it from the tagged commit before publication.

## 5. Audit legs

| Leg | Result |
|---|---|
| Veyra Proof | **REJECTED** (exit 1) at `20261106T151720Z`. `rustfmt`, check, clippy, cargo test, nextest, audit, deny, machete, dylint and veyra-e2e passed. `cargo-llvm-cov` and `cargo-semver-checks` exited 101 with `Too many open files in system`; `cargo-geiger` was INCONCLUSIVE with 1,005 unscanned-file warnings. Evidence: `.veyra/evidence/20261106T151720Z/report.md` (local-only). |
| Rust Audit | exit 1 by threshold: **0 critical / 6 high / 436 medium / 114 low**. All six high are triaged: `pprof` is the real optional profiler dependency (not `approx`); both `build.rs` return before invoking `sh` when `PROFILE=release` (test certificates only in debug); the three `rustc_ast`/`rustc_errors`/`rustc_lint` imports are compiler-private crates used by the Dylint lint crate with `#![feature(rustc_private)]`. Report: `report/rust-audit-20261106-154958.md` (local-only). |
| CodeGraph Rust | **NOT EXECUTED**: index failed with `Ollama embedding API error ... socket: too many open files in system`; the graph was not rebuilt over the final tree, so no query is cited. Log: `/tmp/eqproxy-codegraph.log`. |
| Hindsight | **NOT EXECUTED**: `hindsight-note` reached the local service but it returned `status:error` (`mlx runner failed: missing embedding weight: embed_tokens.weight`). No note is claimed as stored. |
| README release policy | `README.md` contains no EQProxy release version; install commands use `latest`/`releases/latest`. Scout is documented as a hard gate for both architectures. |

The Veyra and CodeGraph failures are environmental file-descriptor
exhaustion on this host, not code findings. They are not presented as passes,
and they block declaring the release verification complete locally.

## 6. What is still required before publication

1. Free the host's file-descriptor pressure and rerun Veyra and CodeGraph over
   the final tree. Veyra is currently **REJECTED** and CodeGraph is
   **NOT EXECUTED**; neither may be described as green.
2. Commit the release tree, then create and push `v0.9.2` only with explicit
   owner authorization. The tag must match `Cargo.toml`.
3. Let the GitHub Actions `release` pipeline build the real multi-stage
   Dockerfile, export the source tarball from the tagged commit, regenerate
   the SBOM, rerun the guard and E2E jobs, and create the GitHub release. The
   source tarball and SBOM are **not** produced from an uncommitted working
   tree.
4. Re-pull the published image, verify the promoted digest and checksums, and
   only then update `CONTRIBUTING.md` with the new published version.
   `README.md` is already version-agnostic and needs no release edit.

No commit, tag, push or release was performed for this preparation.
