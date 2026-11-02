# v0.9.2 — native container images and Ubuntu root/non-root suites

Date: 2026-11-02. Source: working tree at `1ac6ba8` plus the cache and fuzz
changes in this increment. The four suite/image runs below use the code as it
stands in that working tree; the follow-up commit is recorded in the delivery.

This page replaces the earlier QEMU/Alpine-only attempt. None of the results
below was produced under QEMU: each image was built and exercised on a host of
its own architecture.

| Leg | Host | OS / kernel | Docker | Architecture |
|---|---|---|---|---|
| amd64 | `netcup-bench` | Ubuntu 24.04, Linux 6.8 | 29.6.1, buildx 0.35.0 | `x86_64` |
| arm64 | `oracle-quasar` | Ubuntu 24.04, Linux 6.17 | 29.1.3, buildx 0.23.0 | `aarch64` |

Both are the owner's owned Linux evidence hosts (`SERVIDORES.md`). The source
tree was transferred to each host; nothing was built on the macOS arm64 host
and nothing was pushed to a registry.

## 1. Native image builds

Each build names its own native architecture and uses the repository
`docker/Dockerfile`:

```bash
# netcup-bench (x86_64)
docker buildx build --pull --platform linux/amd64 --file docker/Dockerfile \
  --tag eqproxy:native-amd64 --load .

# oracle-quasar (aarch64)
docker buildx build --pull --platform linux/arm64 --file docker/Dockerfile \
  --tag eqproxy:native-arm64 --load .
```

| Image | Architecture | Image ID | Size (inspect) |
|---|---|---|---:|
| `eqproxy:native-amd64` | `linux/amd64` | `sha256:68662b277513ab02a5349853590879a99cccca3004cddb684c2b0e79de91879d` | 20 412 124 B |
| `eqproxy:native-arm64` | `linux/arm64` | `sha256:757d55de0d8e4878538fa25cf9a048cdb21897453c2d5cb7e50d0bab9bee9632` | 18 224 689 B |

The Dockerfile's `file` gate (`statically linked` / `static-pie linked`)
passed in both builds. The image IDs above are local build results; neither
image was published.

## 2. Functional journeys

For each host, the nginx backend was started with the host's native platform:

```bash
docker run -d --network "$NET" --platform linux/amd64 nginx:alpine
docker run -d --network "$NET" --platform linux/amd64 \
  -p 127.0.0.1:18084:18084 -v "$CFG":/c.toml:ro \
  eqproxy:native-amd64 run --config /c.toml
```

The arm64 run used the same commands with `linux/arm64`, port 18085 and
`eqproxy:native-arm64`. The proxy was checked with `--version`, an identity
shell, `config validate`, `config apply` and a real HTTP request to nginx.

| Host | Identity | Version | validate / apply | Live traffic |
|---|---|---|---|---|
| netcup-bench | `x86_64`, uid/gid `65532` | `eqproxy 0.9.1` | OK | `HTTP 200`, `Welcome to nginx!` |
| oracle-quasar | `aarch64`, uid/gid `65532` | `eqproxy 0.9.1` | OK | `HTTP 200`, `Welcome to nginx!` |

The first amd64 identity command used the image entrypoint incorrectly
(`docker run IMAGE sh`), so the image correctly rejected `sh` as an unknown
subcommand. That run is **not** a pass. The journey was repeated with
`--entrypoint sh` and only the corrected run is counted.

The `config apply` plan in both runs reported one HTTP listener on the chosen
port and one backend server pointing at nginx.

## 3. Docker Scout — NOT EXECUTED

Docker Scout CLI `v1.29.0` was installed on both hosts and invoked:

```bash
docker scout cves --only-severity critical,high --exit-code \
  --platform linux/amd64 eqproxy:native-amd64
docker scout cves --only-severity critical,high --exit-code \
  --platform linux/arm64 eqproxy:native-arm64
```

Both commands exited `1` before scanning and answered:

> Log in with your Docker ID or email address to use docker scout.

No Docker Hub credentials were available on the evidence hosts, and none were
copied from the macOS Docker Desktop session. The native Scout gate is therefore
**NOT EXECUTED**. The earlier 0C/0H result over the macOS-built local images is
historical and is not presented as evidence for these native images.

## 4. Ubuntu root and non-root suites

The suites ran on the Netcup Ubuntu 24.04 host with the pinned
`rustc 1.99.1 (58cf8f9c3 2026-10-13)` directly, not in an Alpine container.
Both runs used `ulimit -n 65536` and the CI's `cargo test --workspace --
--nocapture` command. The non-root run used uid 65534 (`nobody`), a copy of the
tree under `/tmp`, and writable `CARGO_HOME`/`CARGO_TARGET_DIR`.

### Non-root

```bash
runuser -u nobody -- env HOME=/tmp/eqproxy-user-home \
  PATH=/tmp/eqproxy-rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin:... \
  CARGO_HOME=/tmp/eqproxy-user-cargo \
  CARGO_TARGET_DIR=/tmp/eqproxy-user-target \
  bash -c 'ulimit -n 65536; cargo test --workspace -- --nocapture'
```

Result: **736 passed, 0 failed, 2 ignored**, process status `0`.
The `panicked` line in the log is the declared `#[should_panic]` test
`await_log_line_times_out_with_the_log`; the same log records
`... should panic ... ok`. The UID gate printed **0 skips** and exited `0`.

Log: `report/native-suite-user.log`; status: `report/native-suite-user.status`.

### Root

The first root run executed the suite but failed one test:

```text
test per_server_maxconn_caps_concurrency_and_answers_503 ... FAILED
assertion `left == right` failed: no extra request may reach the origin past its maxconn
  left: 0
 right: 1
```

That first run had **542 passed, 1 failed, 2 ignored** and status `101`. The
same test was then run alone as root and passed:

```text
test per_server_maxconn_caps_concurrency_and_answers_503 ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

The complete suite was run again as root. Result: **736 passed, 0 failed,
2 ignored**, process status `0`. No test was changed, skipped or weakened.
The failure is recorded as an observed one-off timing failure on the first
full run; the retry and the isolated run are both clean.

Logs: first run `report/native-suite-root-first.log`, retry
`report/native-suite-root.log`; status: `report/native-suite-root.status`.

### UID gates

```bash
python3 .github/scripts/check-uid-skips.py --root-log /tmp/suite-root-retry.log
python3 .github/scripts/check-uid-skips.py --user-log /tmp/suite-user-retry.log
```

Root: **2 printed, 2 declared**, exit `0`. Non-root: **0 printed**, exit `0`.

## 5. Limits

- The images are local build results, not a published multi-arch index. There
  was no registry push, promotion, release workflow or CI run.
- The native suite legs ran on the owner's Ubuntu hosts, not on a GitHub-hosted
  runner. They cover the same Ubuntu 24.04/glibc environment, but they are not a
  GitHub Actions execution.
- The first root run had one observed timing failure. It is not declared as a
  clean run; the retry is clean and the isolated test passes.
- Docker Scout is blocked by missing Docker Hub authentication, as stated above;
  no zero-vulnerability claim is made for the native images.
- This page does not establish production readiness.
