# v0.9.2 — container images, Scout and root/non-root suites

Date: 2026-11-02. Source: `f6ca3b4` on `harden/config-gate-and-errors`.
Host: macOS 26.5 (arm64), Docker Desktop 4.97.0, Engine 29.9.2, buildx 0.38.1,
Docker Scout CLI 1.27.0.

This page records the checks that were still **NOT EXECUTED** in the
`v0.9.2` audit close-out. The images below are local rebuilds; nothing was
pushed or published.

## 1. Images built from the commit

Both builds use the repository Dockerfile with `--pull`:

```bash
docker buildx build --pull --platform linux/arm64 --file docker/Dockerfile \
  --tag eqproxy:local-arm64 --load .
docker buildx build --pull --platform linux/amd64 --file docker/Dockerfile \
  --tag eqproxy:local-amd64 --load .
```

| Image | Architecture | Image ID | Size (inspect) | Runtime user |
|---|---|---|---:|---|
| `eqproxy:local-arm64` | `linux/arm64` | `sha256:c978eaf73bf466c935dde6fa0bebbabf61cd6604e2bda35e2c140ba3962c1a90` | 67 950 478 B | `65532:65532` |
| `eqproxy:local-amd64` | `linux/amd64` | `sha256:5358d9efa04c16dd8b7aa5aff375f2dd1283cb3996c007e10e1bfb7f53b6d5fa` | 77 237 521 B | `65532:65532` |

The Dockerfile's `file` gate (`statically linked` / `static-pie linked`)
passed in both builds. The amd64 build ran under QEMU on this arm64 host; it
proves the artifact, not native performance.

## 2. Functional image journeys

Each image was run with its explicit platform. The checks were:

- `--version` reports `eqproxy 0.9.1`;
- `/bin/sh` reports the expected machine (`aarch64` / `x86_64`) and uid/gid
  `65532`;
- `config validate` accepts the configuration;
- `config apply` writes `/var/lib/eqproxy/current.toml` and prints
  `applied:`;
- live traffic through the image reaches nginx: HTTP `200` with
  `Welcome to nginx!`.

| Image | Version | validate/apply | Live traffic | Log |
|---|---|---|---|---|
| arm64 | OK | OK | `HTTP 200` | `report/docker-e2e-arm64.log` |
| amd64 | OK | OK | `HTTP 200` | `report/docker-e2e-amd64-strict.log` |

The amd64 journey was repeated with `--platform linux/amd64` on **both** the
proxy and the nginx backend after an earlier run omitted the backend flag.

## 3. Docker Scout

```bash
docker scout cves --only-severity critical,high --exit-code \
  --platform linux/arm64 eqproxy:local-arm64
docker scout cves --only-severity critical,high --exit-code \
  --platform linux/amd64 eqproxy:local-amd64
```

Both commands exited `0`. Each image indexed **36 packages** with
**0 critical / 0 high / 0 medium / 0 low**. The full reports are
`report/scout-arm64.log` and `report/scout-amd64.log`.

This is a package-level tripwire, not a proof that the Rust binary contains no
vulnerability, and the local images are not registry-promoted artifacts.

## 4. Non-root and root suites

### Non-root (macOS host)

```bash
cargo test --workspace -- --nocapture
python3 .github/scripts/check-uid-skips.py --user-log /tmp/suite-user.log
```

Result: **735 passed, 0 failed, 2 ignored** (`cargo_test_exit=0`);
`check-uid-skips` printed **0** uid-gated skips and exited `0`.
Logs: `report/suite-user-final.log`, `report/uid-user-check-final.log`.

### Root (Linux/musl container)

The root leg used the `builder` stage, which has the pinned Rust toolchain and
the compiled sources. The final command was:

```bash
docker create --platform linux/arm64 --name eqproxy-root-test-5 \
  -e CARGO_BUILD_JOBS=1 -e CARGO_PROFILE_TEST_DEBUG=0 \
  --volume "$PWD/docs:/src/docs:ro" \
  --volume "$PWD/eqproxy.toml.example:/src/eqproxy.toml.example:ro" \
  --volume "$PWD/eqproxy.schema.json:/src/eqproxy.schema.json:ro" \
  --volume "$PWD/.github:/src/.github:ro" \
  eqproxy:builder-arm64 /bin/sh -c \
    'apk add --no-cache curl openssl && cargo test --workspace -j1 -- --nocapture'
docker start -a eqproxy-root-test-5
python3 .github/scripts/check-uid-skips.py --root-log /tmp/suite-root-5.log
```

Result: **736 passed, 0 failed, 2 ignored** (`cargo_root_test_exit=0`);
`check-uid-skips` printed **2** skips, both declared, and exited `0`.
Logs: `report/suite-root-final.log`, `report/uid-root-check-final.log`.

The container needs `curl` and `openssl` for the real HTTP/2, TLS and SNI E2E
tests, and the tests read `docs/` and `eqproxy.toml.example`; those files are
not in the runtime image by design, so they were mounted read-only. The suite
also needed `-j1` and `CARGO_PROFILE_TEST_DEBUG=0` because the first root run
was `OOMKilled` while linking several static test binaries in parallel. That
was an environment limit; the final run executed the suite normally.

## 5. Limits

- The images are local rebuilds, not the published multi-arch index. There was
  no registry push, promotion or CI run.
- `linux/amd64` execution was emulated by QEMU on this arm64 host.
- Root was exercised in Alpine/musl, not on the GitHub Actions Ubuntu runner;
  the two declared uid-gated skips are expected and were checked by the gate.
- Scout's zero is a package-level result over the 36 packages it indexed; it
  does not scan the Rust binary's own code.
- The allocation measurement remains a count, not a latency benchmark.
