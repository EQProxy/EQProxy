# Profiler dependency fix — inferno 0.12.8 (ADR-0049)

Date: 2026-11-04
Branch: `harden/config-gate-and-errors`
Base: `3d13c14` plus the v0.9.2 working tree

This note records the fix for the Veyra rejection on the optional `profile`
feature, the local validation, the release-runtime validation, and the Linux
container validation of the `profile` image on both architectures.

## Trigger

`veyra audit . --profile standard` returned **REJECTED** (exit 1):

- `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` in `quick-xml 0.26.0`
  (severity 7.5, fixed in >= 0.41.0).
- `cargo-deny` rejected CDDL-1.0 from `inferno 0.11.21`.

Chain: `quick-xml <- inferno 0.11.21 <- pprof 0.15.0 <- eqproxy-cli`
(feature `profile`, never enabled in the release image). The release image
is unchanged; `deny.toml` audits `all-features = true`.

## Change

- `pprof` no longer enables its `flamegraph` feature.
- `inferno 0.12.8` (`default-features = false`, `nameattr`) is a direct
  optional dependency.
- `eqproxy-cli/src/profile.rs` formats the folded stacks from the public
  `pprof::Report::data` and calls `inferno::flamegraph::from_lines`, keeping
  the same SVG and the `PPROF_SVG_BEGIN` / `PPROF_SVG_END` markers.
- `deny.toml` grants CDDL-1.0 to `inferno` only
  (`[[licenses.exceptions]]`), per ADR-0049.
- The `run` loop now handles SIGTERM like SIGINT. The first E2E attempt with
  SIGTERM exited 143 without a dump; `docker stop` sends SIGTERM, so the
  handler is required for the documented profiler workflow.
- The `profile` stage compiles with `CARGO_PROFILE_RELEASE_DEBUG=2`, so the
  release binary keeps symbols for the flamegraph. The `runtime` stage is
  unchanged and carries no debuginfo.
- `write_flamegraph` now returns `Result<bool, String>`; an empty pprof report
  is `Ok(false)`. `finish` logs a warning and does not create a zero-byte SVG
  or emit `PPROF_SVG_BEGIN`/`PPROF_SVG_END`.

## Commands and results

```text
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
exit 0

$ cargo tree --all-features -i quick-xml
quick-xml v0.41.0
└── inferno v0.12.8
    └── eqproxy-cli v0.9.1

$ cargo test -p eqproxy-cli --features profile profile::tests
test profile::tests::flamegraph_svg_contains_the_sampled_frames ... ok
test profile::tests::empty_report_is_not_rendered ... ok
test result: ok. 2 passed; 0 failed

$ cargo test -p eqproxy-cli --test cli_e2e run_shuts_down_gracefully_on_sigterm
test run_shuts_down_gracefully_on_sigterm ... ok
test result: ok. 1 passed; 0 failed
```

Real binary with the `profile` feature (`target/debug/eqproxy`, config with a
loopback HTTP origin, `EQPROXY_PPROF_DIR` set, `EQPROXY_PROFILE_SAMPLES=1`):

| Signal | HTTP 200 | Exit | SVG | Markers | Samples |
|---|---|---|---|---|---|
| SIGTERM before the fix | 20/20 | 143 | none | 0/0 | 3 |
| SIGINT (render check) | 20/20 | 0 | 114 347 bytes, 2 `<svg`, EQProxy frames | 1/1 | 3 |
| SIGTERM after the fix | 20/20 | 0 | 89 007 bytes, 2 `<svg`, EQProxy frames | 1/1 | 2 |

The SIGINT run used the same renderer and proved the inferno 0.12 path before
the signal fix; the SIGTERM run proves the workflow `docker stop` relies on.

## Linux container validation (2026-11-04)

The `profile` stage was rebuilt from a clean tree at `84cba4b` plus only the
profiler fixes, with explicit `--target profile` and
`CARGO_PROFILE_RELEASE_DEBUG=2`. The explicit target matters: the Dockerfile
ends at `FROM runtime AS profile`, so a default build produces the `profile`
stage, including `MIMALLOC_SHOW_STATS=1`.

```text
docker buildx build --file docker/Dockerfile --target profile \
  --build-arg PROFILE=1 --platform linux/arm64 \
  --tag eqproxy-profile:v093-arm64 --load .
docker buildx build --file docker/Dockerfile --target profile \
  --build-arg PROFILE=1 --platform linux/amd64 \
  --tag eqproxy-profile:v093-amd64 --load .
```

Each run sent 150 HTTP/1.1 requests, then `docker kill -s SIGINT` or
`-s SIGTERM`, waited for exit, and copied `/tmp/eqproxy-profile` out of the
container. The config used `shutdown_grace = "5s"` and the origin container's
IP address (the proxy's hostname resolution did not reach the Docker DNS in
this setup).

| Arch | Signal | HTTP 200 | Exit | SVG | Markers | Symbolization |
|---|---|---|---|---|---|---|
| linux/arm64 | SIGINT | 150/150 | 0 | valid, 2 `<svg` | 1/1 | `tokio` / `core` / `std` frames |
| linux/arm64 | SIGTERM | 150/150 | 0 | valid, 2 `<svg` | 1/1 | `tokio` / `core` / `std` frames |
| linux/amd64 | SIGINT | 150/150 | 0 | valid, 2 `<svg` | 1/1 | address frames; likely the wasmtime JIT |
| linux/amd64 | SIGTERM | 150/150 | 0 | valid, 2 `<svg` | 1/1 | address frames; likely the wasmtime JIT |

Images: `sha256:f8dd33710ebddb71d2ce3a252a6644cd990a82e6bf68d553b710b24f60881e23`
(linux/arm64, 553 285 633 B) and
`sha256:fbe806fd79e3a72d8fa0eb61853c4917020edc1ae864e7d67abd581e34aa2509`
(linux/amd64, 593 022 104 B). These are development images and are not
published; Scout was run on the release `runtime` images, not on these
`profile` images.

## Release runtime validation (2026-11-04)

Both native runtime images were built with `--target runtime`, loaded, and
exercised with the release E2E harness (100 HTTP/1.1 requests, nginx origin,
`validate` exit 0, SIGTERM shutdown):

| Arch | Image | Image ID | Size | HTTP 200 | Exit | Scout |
|---|---|---|---:|---|---|---|
| linux/amd64 | `eqproxy-runtime:v093-amd64` | `sha256:9df1c975c2ee5f66c3b16323c7749646902a81cb2d544ed3db6bb25c90a52a17` | 20 410 054 B | 100/100 | 0 | 0C/0H/0M/0L |
| linux/arm64 | `eqproxy-runtime:v093-arm64` | `sha256:be17ec2f90741aaf357f04423990e8a605637c9655cea60a25af00abd83eaf02` | 67 898 523 B | 100/100 | 0 | 0C/0H/0M/0L |

The binary reports `eqproxy 0.9.1`; `validate` exits 0 on both images. The
arm64 image was loaded from the Oracle-built tar (`sha256:992b38645d456cdbeaddaf9bc56c203bb4dcb927b92983cd87f9256acada9e40`).
Nothing was published.

## S-H2-01 re-measurement after the 16 KiB change (2026-11-04)

The release runtime images were also measured with five repetitions of
S-H2-01 on the native hosts. The change is a memory-bounded default, not a
throughput win:

| Host | EQProxy RPS | HAProxy RPS | EQ/HA | EQ p50 / p99 ms | EQ / HA peak MiB | Errors |
|---|---:|---:|---:|---:|---:|---:|
| netcup amd64 | 34 376.5 | 57 904.5 | 0.594 | 11.41 / 21.09 | 49.52 / 39.39 | 0 |
| Oracle arm64 | 18 707.1 | 25 130.2 | 0.744 | 21.26 / 37.35 | 48.98 / 35.46 | 0 |

Both cells remain **Worse** on RPS and p50; peak memory fell versus the 64 KiB
campaign (50.37→49.52 MiB netcup, 52.46→48.98 MiB Oracle), so the 16 KiB
`PIPE_BUFFER` stays for memory, without an RPS claim. Ten raw files per host
were checksum-verified with zero mismatches; the evidence is under
`docs/evidence/v092-bench-h2/run-20261104T182135Z` and
`.../run-20261104T182404Z`.

## Limits

- The macOS/aarch64 debug run remains the local signal/render check; the
  Linux container table above is the image validation.
- Release `runtime` was rebuilt and validated on both architectures: E2E
  100/100, exit 0, Scout 0C/0H/0M/0L. It is not published and its image IDs are
  the artifact identity above.
- The `profile` stage is optional, development-only and not published. It
  includes debuginfo, so its images are 553 MB (arm64) and 593 MB (amd64);
  Docker Scout was not used to claim coverage for those images.
- Docker Scout indexes the Wolfi base packages; the statically linked Rust
  dependencies are covered by `cargo deny`, not by that package index.
- Symbolization: arm64 resolves `tokio`/`core`/`std` frames on both signals;
  amd64 still shows address frames, consistent with the wasmtime JIT. The unit
  test covers the folded-stack renderer with named frames.
- The first 20-request SIGTERM arm64 run produced a zero-byte SVG because the
  pprof report had no CPU samples. The empty-report path now returns
  `Ok(false)`, creates no SVG and emits no markers; `empty_report_is_not_rendered`
  covers it. The 150-request runs are the validated shape.
- Veyra re-ran over the code tree: **ACCEPTED**, exit 0, evidence
  `.veyra/evidence/20261104T182457Z/`; `geiger` was INCONCLUSIVE. Rust Audit
  exited 1 under `--fail-on high` with 0 critical / 12 high / 430 medium / 114
  low; the report is `report/rust-audit-20261104-192529.md`.
- No commit, push, tag or release was made for these new fixes.
