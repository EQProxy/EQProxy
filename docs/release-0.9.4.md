# v0.9.4 — publication record

Language: **English**.

**What this page answers:** what `v0.9.4` published, from which tag and commit,
with which source tarball, test evidence, Scout results and image digests. The
figures below were checked against the published GitHub release, its assets and
the Release run on 2026-11-09. This page is a record of the published `v0.9.4`
tag; it does not move the tag or create a new release.

- Version: **0.9.4** (`Cargo.toml` `[workspace.package] version`).
- Tag: annotated object `da14ee23d6faa1d9f16ec72a0853bf99afd1154a` over commit
  `24e6dc6b29728b8ce16c197642a58268a27d89bd` (`fix(net): close M-01 HTTP/1.0
  framing for v0.9.4`), message `EQProxy v0.9.4`. The remote carries the same
  tag object.
- Release: [v0.9.4](https://github.com/EQProxy/EQProxy/releases/tag/v0.9.4),
  published `2026-11-09T00:50:07Z`, not a draft and not a pre-release, with
  **10 assets**. The GitHub “latest release” API also resolves to `v0.9.4`.
- Pipeline run: [42528272498](https://github.com/EQProxy/EQProxy/actions/runs/42528272498)
  (`Release`, event `push`, branch `v0.9.4`, head `24e6dc6`), `completed`,
  conclusion `success`, attempt `1`.
- Release notes: [CHANGELOG.md](../CHANGELOG.md), section
  `## [v0.9.4] — 2026-11-08`.
- Pipeline: [.github/workflows/release.yml](../.github/workflows/release.yml).

The tag points at the release commit, not at the branch head. This record is a
later documentation commit on the branch and is not part of the tagged tree;
the only changes between `v0.9.4` and this record's commit are documentation
(`CONTRIBUTING.md` and this page). The tarball, tests and image evidence below
are from the tagged tree.

## 1. Source tarball

The release job exported `eqproxy-0.9.4-src.tar.gz` from the tagged commit.
Its GitHub asset digest and its downloaded bytes agree:

| Property | Value |
|---|---|
| Asset | `eqproxy-0.9.4-src.tar.gz` |
| Size | 6 714 904 bytes |
| SHA-256 | `9c011e86e684c04209549973f493560ea3617911162fd197dc9cefe1e9f56621` |
| Workspace version inside | `0.9.4` |
| Archive entries | 4 024 |

The archive contains no `ROADMAP EQPROXY/`, `SERVIDORES.md`, `.dockerignore`,
`scripts/`, `report/`, `AGENTS.md`, `USER_PAIN_POINTS.md`, `PARITY_MATRIX.md`,
`.gitignore`, `.cursor/` or local `wizard-*.sh`. The tarball declares the same
workspace version:

```console
$ tar -xOzf eqproxy-0.9.4-src.tar.gz eqproxy-0.9.4/Cargo.toml | sed -n '1,7p'
[workspace]
resolver = "2"
members = ["eqproxy-core", "eqproxy-config", "eqproxy-net", "eqproxy-cli"]

[workspace.package]
version = "0.9.4"
```

`SHA256SUMS.txt` lists the two Linux binaries and the source tarball; checking
all three downloaded files succeeds:

```console
$ sha256sum -c SHA256SUMS.txt
eqproxy-linux-amd64: OK
eqproxy-linux-arm64: OK
eqproxy-0.9.4-src.tar.gz: OK
```

### Published asset digests

The `SHA-256` values below are the digests reported by GitHub for the uploaded
assets and match the downloaded files in the checksummed set.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `eqproxy-0.9.4-src.tar.gz` | 6 714 904 | `9c011e86e684c04209549973f493560ea3617911162fd197dc9cefe1e9f56621` |
| `eqproxy-linux-amd64` | 37 295 336 | `53f6d2eb93f670dc0f7bce2296af47b0f80a897fde136128d63e84c17250cd9d` |
| `eqproxy-linux-arm64` | 31 772 576 | `d0a68be439616dbc9983bb4516569f87355aab100662d9244818899a1fb32e16` |
| `eqproxy-cli.cdx.json` | 237 916 | `2568a95c21da488cc73e5619c6fdd845ad7f774aacf9f33e91e750b1b4ef167e` |
| `eqproxy-config.cdx.json` | 59 576 | `44053357f93e88f782879a4051bfc39f02dc105871bb11abc50c988f5e08904d` |
| `eqproxy-core.cdx.json` | 36 560 | `b3307c4c58d21651701256927d4957425b371efc70be086640befbc1fb673f7c` |
| `eqproxy-net.cdx.json` | 205 755 | `2884339d27d5101a653de0435a42c6abe4b5a3ac98ffdbb5a700de0bb688f3af` |
| `SCOUT-0.9.4-linux-amd64.md` | 1 174 | `c368bed1380b096fc965b869c5fd54de16422803b5c1b973dff268b39b936f99` |
| `SCOUT-0.9.4-linux-arm64.md` | 1 174 | `4963ca835c078a4ae95636dbd377ba0f14b035c59d47da1aff2c0158b411f888` |
| `SHA256SUMS.txt` | 263 | `7616dfbd1f5b61907055e69dab4b3a586ec6c56ac3e7e9bbe64c940336630203` |

The two Linux binaries are static or static-pie executables of the expected
architectures:

```console
$ file eqproxy-linux-amd64 eqproxy-linux-arm64
eqproxy-linux-amd64: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV),
  static-pie linked, BuildID[sha1]=4102adec2d3ea862bb2af7d718b3579697195398, not stripped
eqproxy-linux-arm64: ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV),
  statically linked, BuildID[sha1]=099acd323f19f8fd2917b29425d38eeec7d01275, not stripped
```

## 2. M-01 in the release tree

M-01 is the HTTP/1.0 framing gap: an HTTP/1.0 request must not use
`Transfer-Encoding: chunked`, and an HTTP/1.0 response cannot advertise chunked
framing. The tagged tree implements both halves:

- `eqproxy-net/src/http1.rs`: rejects `Transfer-Encoding: chunked` on an
  HTTP/1.0 request with `400 transfer-encoding is not valid in HTTP/1.0`;
- `eqproxy-net/src/http1.rs`: dechunks a chunked upstream response for an
  HTTP/1.0 client and sends it close-delimited, without `Transfer-Encoding` or
  `Trailer`;
- the unit test `chunked_transfer_encoding_is_rejected_in_http10` covers the
  request rejection;
- `eqproxy-net/tests/e2e_http1.rs` covers both the normal HTTP/1.0
  close-delimited response and the HEAD response that must not advertise
  chunked framing.

### Tests from the published tarball

The tarball was extracted and its tests were run without modifying the source:

```console
$ cargo test -p eqproxy-net --lib chunked_transfer_encoding_is_rejected_in_http10
test http1::tests::chunked_transfer_encoding_is_rejected_in_http10 ... ok
test result: ok. 1 passed; 0 failed; 241 filtered out

$ cargo test -p eqproxy-net --test e2e_http1 http10_
test http10_head_response_never_advertises_chunked_framing ... ok
test http10_client_gets_dechunked_close_delimited_response ... ok
test result: ok. 2 passed; 0 failed; 23 filtered out
```

The complete workspace suite also ran from that extracted tarball:

```console
$ cargo test --workspace
test result: ok. … 0 failed …
760 passed; 0 failed; 2 ignored
```

The run covered 37 test-result sections. Exit status was **0**.

### Regression proof without the fix

The same M-01 E2E tests applied to the parent tree `dc184f3` — which does not
contain the fix — failed both scenarios:

```console
test result: FAILED. 0 passed; 2 failed
http10_client_gets_dechunked_close_delimited_response ... FAILED
http10_head_response_never_advertises_chunked_framing ... FAILED
exit status 101
```

That failure is the negative control: the tests fail without the change and
pass in the published tree.

### Pre-publication gates

The workspace was also checked with:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit **0** |
| `cargo build --workspace --release` | exit **0** |

The source tarball itself is tested above; the published Linux binaries were
built by the Release workflow, not by that local tarball test run.

## 3. Docker Scout

The image job built the candidate index, scanned both architectures, exercised
the image and promoted the scanned digest. The two published reports name the
same candidate index, `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8`:

| | `linux/amd64` | `linux/arm64` |
|---|---|---|
| report asset | `SCOUT-0.9.4-linux-amd64.md` | `SCOUT-0.9.4-linux-arm64.md` |
| platform manifest digest | `sha256:265b3cd3734c1230e5b370b50d3e33127bfb32b002807df91c9831d56ba00b4d` | `sha256:234fe266bc45fe2e140b36154914176e7f1e0a04ade6e71eb8a5d523c42377e8` |
| critical / high / medium / low | **0 / 0 / 0 / 0** | **0 / 0 / 0 / 0** |
| packages | 69 | 69 |
| size | 20 MB | 18 MB |

The reports were produced with:

```console
$ docker scout cves --format markdown --platform linux/amd64 \
    ghcr.io/eqproxy/eqproxy:0.9.4-candidate
$ docker scout cves --format markdown --platform linux/arm64 \
    ghcr.io/eqproxy/eqproxy:0.9.4-candidate
```

Both reports record zero findings in every severity bucket. The workflow's
gate then runs `docker scout cves --only-severity critical,high --exit-code`
for each architecture, so a nonzero Scout result would fail the image job. The
reports show 0 critical / 0 high and run [42528272498](https://github.com/EQProxy/EQProxy/actions/runs/42528272498)
concluded `success`.

## 4. Published image digests

The candidate index that was scanned is the index that was promoted, so the
release tags and the mirror refer to the same bytes:

| Reference / component | Digest |
|---|---|
| published index (`ghcr.io/eqproxy/eqproxy:0.9.4`) | `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8` |
| `linux/amd64` manifest | `sha256:265b3cd3734c1230e5b370b50d3e33127bfb32b002807df91c9831d56ba00b4d` |
| `linux/arm64` manifest | `sha256:234fe266bc45fe2e140b36154914176e7f1e0a04ade6e71eb8a5d523c42377e8` |

The Release workflow re-pulled `ghcr.io/eqproxy/eqproxy:0.9.4` and
`exyonq/eqproxy:0.9.4` for both architectures and replayed version, digest,
HTTP/2 gate, config-apply and live-traffic journeys; all four combinations
reported success. The tag is **not** moved by this record, and the evidence
above belongs to the published `v0.9.4` artifacts.

## 5. Closing recheck (2026-11-09)

The final check was run from a detached worktree at `refs/tags/v0.9.4`, not
from the branch:

- annotated tag object `da14ee23d6faa1d9f16ec72a0853bf99afd1154a`, target
  `24e6dc6b29728b8ce16c197642a58268a27d89bd`;
- `Cargo.toml` and `Cargo.lock` report `0.9.4`;
- `CHANGELOG.md` contains `## [v0.9.4] — 2026-11-08` at line 8;
- M-01 focused tests passed: **1 unit + 2 E2E**;
- `cargo test --workspace --locked` passed: **760 passed, 0 failed, 2 ignored**,
  exit `0`;
- the release still has the exact 10-asset set listed in section 1, all
  `uploaded`;
- `v$VER == $GITHUB_REF_NAME` holds for `v0.9.4` and workspace `0.9.4`.

The two `0.9.4` registry references were inspected without pulling:

```console
$ docker buildx imagetools inspect --raw ghcr.io/eqproxy/eqproxy:0.9.4
$ docker buildx imagetools inspect --raw exyonq/eqproxy:0.9.4
```

| Reference | Index digest | `linux/amd64` | `linux/arm64` |
|---|---|---|---|
| `ghcr.io/eqproxy/eqproxy:0.9.4` | `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8` | `sha256:265b3cd3734c1230e5b370b50d3e33127bfb32b002807df91c9831d56ba00b4d` | `sha256:234fe266bc45fe2e140b36154914176e7f1e0a04ade6e71eb8a5d523c42377e8` |
| `exyonq/eqproxy:0.9.4` (Docker Hub) | `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8` | `sha256:265b3cd3734c1230e5b370b50d3e33127bfb32b002807df91c9831d56ba00b4d` | `sha256:234fe266bc45fe2e140b36154914176e7f1e0a04ade6e71eb8a5d523c42377e8` |

Both registries resolve to the same index and the same two platform manifests.
No `:latest` tag was pulled or rechecked for this record. The tag and release
were not modified; this section records the recheck, not a new publication.

## 6. Limitations

- The source tarball was extracted, checksum-verified and tested, but the
  published Linux binaries were not rebuilt from that export; they are the
  workflow's build output.
- Scout evidence is the two reports published with the release. A local
  full-image rebuild was attempted during preparation but did not complete
  within the local command limit, so no local build is cited as release
  evidence.
- The release is Linux-only: there are no Darwin assets in `v0.9.4`.
