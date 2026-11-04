# Profiler dependency fix — inferno 0.12.8 (ADR-0049)

Date: 2026-11-04
Branch: `harden/config-gate-and-errors`
Base: `3d13c14` plus the v0.9.2 working tree

This note records the fix for the Veyra rejection on the optional `profile`
feature, the local validation, and the Linux container validation of the
`profile` image on both architectures.

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
test result: ok. 1 passed; 0 failed

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

The `profile` stage was built from this working tree with Buildx for both
architectures and exercised against an `nginx:alpine` origin on a dedicated
Docker network:

```text
docker buildx build --file docker/Dockerfile --target profile \
  --build-arg PROFILE=1 --platform linux/arm64 \
  --tag eqproxy-profile:v092-arm64 --load .
docker buildx build --file docker/Dockerfile --target profile \
  --build-arg PROFILE=1 --platform linux/amd64 \
  --tag eqproxy-profile:v092-amd64 --load .
```

Each run sent 150 HTTP/1.1 requests, then `docker kill -s SIGINT` or
`-s SIGTERM`, waited for exit, and copied `/tmp/eqproxy-profile` out of the
container. The config used `shutdown_grace = "5s"` and the origin container's
IP address (the proxy's hostname resolution did not reach the Docker DNS in
this setup).

| Arch | Signal | HTTP 200 | Exit | SVG | Markers | `PPROF_SAMPLE` logs |
|---|---|---|---|---|---|---|
| linux/arm64 | SIGINT | 150/150 | 0 | 27 082 B, 2 `<svg`, `eqproxy_net` frames | 1/1 | 8 |
| linux/arm64 | SIGTERM | 150/150 | 0 | 18 967 B, 2 `<svg`, mostly address frames | 1/1 | 8 |
| linux/amd64 | SIGINT | 150/150 | 0 | 19 549 B, 2 `<svg`, mostly address frames | 1/1 | 10 |
| linux/amd64 | SIGTERM | 150/150 | 0 | 19 173 B, 2 `<svg`, mostly address frames | 1/1 | 10 |

Images: `sha256:4f3a664275a1b7026cb705e5cdc70df0a3322fb706c06bfc733083d567119a8e`
(linux/arm64, 69 172 972 B) and
`sha256:bbe0b09fa0c2fc55a0f7b4d5f5e772de69dcac4f0934868fd5cdb94ae9ebc6d2`
(linux/amd64, 77 724 539 B).

`docker scout cves <image> --only-severity critical,high --exit-code` returned
exit 0 and `0C 0H 0M 0L` / "No vulnerable packages detected" for both images
(36 packages indexed per image).

The first 20-request SIGTERM arm64 run produced a zero-byte SVG: the pprof
report had no CPU samples, and `write_flamegraph` returns `Ok(())` for empty
input. The harness rejected it (`SVG_BYTES=0`); rerunning with 150 requests
produced a valid SVG. The empty-report path remains a diagnostic gap: `finish`
logs "written" even when the line set is empty.

## Limits

- The macOS/aarch64 debug run remains the local signal/render check; the
  Linux container table above is the image validation.
- The release image was not rebuilt or scanned; only the optional `profile`
  stage is covered, and nothing is published.
- Docker Scout indexes the Wolfi base packages (36 per image); the statically
  linked Rust dependencies are covered by `cargo deny`, not by that package
  index.
- Symbolization is partial in the static release binary: arm64 SIGINT resolved
  `eqproxy_net` frames, while the other runs show mostly address frames. The
  unit test covers the folded-stack renderer with named frames.
- The first 20-request SIGTERM arm64 run produced a zero-byte SVG because the
  pprof report had no CPU samples; the harness rejected it. The 150-request
  runs above are the validated shape. `finish` still logs "written" for an
  empty report, which is a diagnostic gap.
- The Veyra re-run over the code tree is reported in the delivery; this note
  was updated afterwards with the container results.
