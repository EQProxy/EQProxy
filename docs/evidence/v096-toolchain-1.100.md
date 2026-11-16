# v0.9.6 — Rust toolchain 1.100.0

Date: 2026-11-14. Branch `release/v0.9.5`, tree at `ae85cf9` plus the
pre-existing uncommitted work. This increment updates the **project toolchain
pin**; it does not update, move or publish a release tag. No commit, push, tag,
release or workflow dispatch was performed.

Rust 1.100.0 was published as stable on 2026-11-12:
<https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/>. The pin is an exact
release, not `stable`, so CI, local builds and the image builder resolve the
same compiler on every run.

## 1. Changes

| File | Change |
|---|---|
| `rust-toolchain.toml` | `channel = "1.99.1"` → `channel = "1.100.0"` |
| `docker/Dockerfile` | `ARG RUST_VERSION=1.99.1` → `1.100.0` |
| `CONTRIBUTING.md` | the current-pin sentence now says `1.100.0` |
| `AGENTS.md` (local-only, untracked) | the pin fact now says `1.100.0` |

The two pins are equal and the CI gate that compares them passes:

```console
$ sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml
1.100.0
$ sed -n 's/^ARG RUST_VERSION=\(.*\)$/\1/p' docker/Dockerfile
1.100.0
$ test "$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)" = \
       "$(sed -n 's/^ARG RUST_VERSION=\(.*\)$/\1/p' docker/Dockerfile)"
$ echo $?
0
```

## 2. Compiler actually used

`rustup show active-toolchain` selects the checkout's override:

```console
1.100.0-aarch64-apple-darwin (overridden by '<repo>/rust-toolchain.toml')
rustc 1.100.0 (0ef1bc386 2026-11-09)
cargo 1.100.0 (3acc79c0c 2026-11-05)
```

`rustup check` reports `1.100.0 (0ef1bc386 2026-11-09)` as the available stable
release. The project override, not the machine's default alias, is what every
command below used.

## 3. Local gates (macOS arm64)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 0 — **847 passed, 0 failed, 2 ignored**, 41 result blocks |
| `cargo build --workspace --release --locked` | exit 0 — release profile finished in 2m 14s |
| `cargo check --manifest-path fuzz/Cargo.toml --bins --locked` | exit 0 — fuzz workspace checked with 1.100.0 |
| `git diff --check` | exit 0 |

The two ignored tests are the existing Docker/reference-image harnesses
(`differential_shared_semantics_corpus` and
`migration_matrix_same_routing_as_the_originals`); they were not executed in
this increment. The suite output is `/tmp/eqproxy-rust-1.100-tests.log`; the
release build log is `/tmp/eqproxy-rust-1.100-release-build.log`.

Local release artifact:

```console
$ target/release/eqproxy --version
eqproxy 0.9.6
$ shasum -a 256 target/release/eqproxy
86de45fc7f5b5c9fc48c23f7ce0950c08d14bacd9ff44ae81c3a071c36a8cb7c
```

## 4. Release-image builder and Linux artifact

`docker manifest inspect rust:1.100.0-alpine` exits 0 and resolves an OCI image
index with `linux/amd64` (`sha256:cfbeb9c8c956f048a081ae034a7c8ffbd60242f3ff76b645d3a41523f665f761`)
and `linux/arm64` (`sha256:18b6dc7bc111bde4e2fccf6c4ec95cc4a3486b41e4b090e9b0202eba9540023e`)
manifests, so the Dockerfile's tag is not a guessed or removed tag.

The `export` stage was built for `linux/arm64` with
`--build-arg RUST_VERSION=1.100.0`. The first foreground attempt was cut off by
the desktop command's 295 s wait limit while the layer was still compiling;
the rerun in the background completed. It used the Alpine builder and the
release profile:

```console
#20 3.542    Compiling aws-lc-rs v1.18.2
#20 6.699    Compiling aws-lc-sys v0.46.0
#20 564.1    Finished `release` profile [optimized] target(s) in 9m 23s
#20 DONE 564.3s
#21 [export 1/1] COPY --from=builder /opt/eqproxy /eqproxy
#22 DONE 0.6s
```

Artifact and execution on Linux:

```console
$ file /tmp/eqproxy-rust-1.100-export/eqproxy
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked,
BuildID[sha1]=a8b97cba28060087073d819a106044a5ff01d867, not stripped
$ shasum -a 256 /tmp/eqproxy-rust-1.100-export/eqproxy
73bfb927fb81232b76599bbd2da2ec5fd94bad704e310c68da38dd05af246452
$ docker run --rm --platform linux/arm64 \
    -v /tmp/eqproxy-rust-1.100-export:/out:ro alpine:latest /out/eqproxy --version
eqproxy 0.9.6
```

The build log is `/tmp/eqproxy-rust-1.100-docker-build.log`; its exit code is
stored in `/tmp/eqproxy-rust-1.100-docker.rc` (`0`).

## 5. Other gates

| Command | Result |
|---|---|
| `.github/scripts/check-short-health-probes.py --self-test` | self-test ok |
| `.github/scripts/check-short-health-probes.py` | exit 0 — every suite health probe has a timeout or a declared reason |
| `.github/scripts/check-evidence-test-citations.py --self-test` | self-test ok |
| `.github/scripts/check-evidence-test-citations.py` | exit 0 — 146 documents, 257 cited identifiers, 834 tests in 87 files; existing non-test-helper notes only |

## 6. Limits and NOT EXECUTED

- **No publication:** no commit, push, tag, GitHub release, image push or
  workflow run. The release workflow's remote jobs and Docker Scout are
  **NOT EXECUTED**.
- **Only linux/arm64 was built.** The amd64 image and the multi-architecture
  manifest were **NOT EXECUTED**; the Docker manifest check proves the base
  tag exists for both, not that this tree builds them.
- **No container proxy E2E:** the Linux artifact was executed for its version,
  but no HTTP/TCP traffic was driven through it inside the container in this
  increment. The 847-test suite ran on the host with real sockets, not inside
  the image.
- **Existing ignored harnesses:** differential parity and migration were
  **NOT EXECUTED** because they need Docker and the reference images.
- **Fuzz campaigns:** the fuzz workspace was compiled, but no libFuzzer
  campaign was run.
- **Historical documents are not rewritten.** Pages that record earlier
  artifacts and runs, such as `docs/evidence/v08-toolchain-pin.md`,
  `docs/evidence/v095-release.md`, `docs/release-0.9.5.md`,
  `docs/parity-and-evidence.md` and the 2026-11-09 audit logs, still cite
  1.99.1 because that is what they measured. They remain historical evidence,
  not current pins.

No code or dependency changed. The Cargo lockfiles were not modified by this
increment; `--locked` passed for both workspaces.

## 7. Final state check

After the Docker build, external edits were observed restoring
`channel = "1.99.1"` in `rust-toolchain.toml` and `(currently 1.99.1)` in
`CONTRIBUTING.md` while the Dockerfile still said `1.100.0`. Both files were
corrected to `1.100.0` and re-checked after a delay: all three pins match,
`rustup show active-toolchain` selects `1.100.0`, `cargo --version` reports
`1.100.0`, and `cargo fmt --all -- --check` exits 0 again. Every build and test
above ran with `1.100.0` active; the corrections only restored the pins the
checks used.

The closure note was stored in the EQProxy Hindsight bank with `sync_retain`
under tags `eqproxy`, `toolchain`, `rust-1.100.0` and `v0.9.6`; it returned 11
memory units, the first being `4f007b8e-1629-4d13-a8ea-1eb6f7dbff64`. The recall
of the `toolchain`, `docker` and `release` areas returned exit 2 (no memories)
before this increment, which is **NOT EXECUTED**, not a claim of "no known
pitfalls".
