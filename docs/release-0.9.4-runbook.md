# EQProxy v0.9.4 — publication runbook

Status as of **2026-11-09**: the release is already published. This runbook
records the exact sequence, what is already true, and what was **not** executed
in this turn. Do not recreate or move the `v0.9.4` tag without an explicit owner
decision.

## 1. Verified state

| Fact | Value |
|---|---|
| Release commit | `24e6dc6b29728b8ce16c197642a58268a27d89bd` (`fix(net): close M-01 HTTP/1.0 framing for v0.9.4`) |
| Annotated tag object | `da14ee23d6faa1d9f16ec72a0853bf99afd1154a` → commit `24e6dc6…89bd` |
| Branch | `release/v0.9.4` at `ecda55a46f91982842f8bfe87772bf44324df905` (3 docs-only commits after the tag) |
| `v0.9.3` ancestry | `git merge-base --is-ancestor v0.9.3^{commit} v0.9.4^{commit}` → exit 0 |
| Workspace version | `Cargo.toml` `[workspace.package] version = "0.9.4"`; all four crates `0.9.4` |
| Lockfile | `Cargo.lock` has `eqproxy-cli`, `eqproxy-config`, `eqproxy-core`, `eqproxy-net` at `0.9.4` |
| Changelog | `CHANGELOG.md` has `## [v0.9.4] — 2026-11-08`; the workflow extractor produces `## EQProxy v0.9.4 — 2026-11-08` |
| Code vs tag | `git diff --name-only v0.9.4..HEAD -- '*.rs' Cargo.toml Cargo.lock '*/Cargo.toml'` is empty |
| Remote tag | `refs/tags/v0.9.4` and `refs/tags/v0.9.4^{}` exist on `origin` |
| GitHub Release | `v0.9.4`, published (`isDraft=false`, `isPrerelease=false`), 10 assets |
| GHCR | `ghcr.io/eqproxy/eqproxy:0.9.4` index `sha256:d139f07b49f6829e354971ade4076512eae469cf40540c4e34add61fd0574ba8` |
| Docker Hub | `docker.io/exyonq/eqproxy:0.9.4` same index digest |

The only commits after the tag are `docs/release-0.9.4.md` and `CONTRIBUTING.md`;
they do not change the release tree.

## 2. M-01 regression evidence

M-01 is the HTTP/1.0 framing gap: an HTTP/1.0 request must not use
`Transfer-Encoding: chunked`, and an HTTP/1.0 response cannot advertise chunked
framing.

**Red before the fix** — detached worktree at `dc184f3`, with the test file from
the fixed tree copied in, so the same regression test runs against the old
product code:

```console
$ git worktree add --detach /tmp/eqproxy-m01-parent dc184f3
$ cp eqproxy-net/tests/e2e_http1.rs /tmp/eqproxy-m01-parent/eqproxy-net/tests/e2e_http1.rs
$ cd /tmp/eqproxy-m01-parent
$ CARGO_TARGET_DIR=/tmp/eqproxy-m01-target \
    cargo test -p eqproxy-net --test e2e_http1 http10_ -- --test-threads=1
...
test http10_client_gets_dechunked_close_delimited_response ... FAILED
test http10_head_response_never_advertises_chunked_framing ... FAILED
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 23 filtered out
exit status 101
```

**Green after the fix** — tagged release tree:

```console
$ cargo test -p eqproxy-net --lib chunked_transfer_encoding_is_rejected_in_http10
test http1::tests::chunked_transfer_encoding_is_rejected_in_http10 ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 241 filtered out

$ cargo test -p eqproxy-net --test e2e_http1 http10_
test http10_head_response_never_advertises_chunked_framing ... ok
test http10_client_gets_dechunked_close_delimited_response ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out
```

The parent worktree and its target directory are temporary and can be removed:

```bash
git worktree remove --force /tmp/eqproxy-m01-parent
rm -rf /tmp/eqproxy-m01-target
```

## 3. Pre-publication gates (this turn, tagged tree)

| Command | Exit | Result |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | no warnings |
| `cargo test --workspace --locked` | 0 | 37 result sections: **760 passed, 0 failed, 2 ignored** |
| `cargo build --workspace --release --locked` | 0 | release build |
| `python3 .github/scripts/check-secrets.py --self-test` / full | 0 / 0 | self-test OK; no secrets in tracked files |
| `python3 .github/scripts/check-short-health-probes.py --self-test` / full | 0 / 0 | 18 probes, 2 declared |
| `python3 .github/scripts/check-docker-build-target.py --self-test` / full | 0 / 0 | 3 image builds + 1 export build use the expected stage |
| `python3 .github/scripts/check-evidence-test-citations.py --self-test` / full | 0 / 0 | every cited test-shaped identifier exists |
| `python3 .github/scripts/check-uid-skips.py --self-test` | 0 | self-test OK |

The release workflow's tag/version guard also passes locally:

```bash
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
test "v$VER" = "v0.9.4"   # exit 0
```

## 4. Publication sequence (already completed)

These are the exact commands in order. **Every step below was already executed
before this turn; none was executed again here.** They are kept for
reproducibility and for the next release after replacing `0.9.4`/`v0.9.4`.

```bash
# 0. Start from the release branch, not from an old HEAD.
git switch release/v0.9.4
git merge-base --is-ancestor v0.9.3^{commit} HEAD   # must be 0

# 1. Version + changelog + code are already in the release commit.
grep -n '^version = "0.9.4"' Cargo.toml
grep -n '^## \[v0.9.4\]' CHANGELOG.md

# 2. Gates (run on the exact tree that will be tagged).
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked

# 3. Release commit (already 24e6dc6).
git status --short
git add Cargo.toml Cargo.lock CHANGELOG.md README.md docker/Dockerfile \
  eqproxy-core/Cargo.toml eqproxy-config/Cargo.toml eqproxy-net/Cargo.toml \
  eqproxy-cli/Cargo.toml eqproxy-cli/tests/cli_e2e.rs \
  eqproxy-net/src/http1.rs eqproxy-net/src/http2.rs \
  eqproxy-net/tests/e2e_http1.rs eqproxy-net/tests/framing_fuzz_corpus.rs \
  fuzz/Cargo.lock
git commit -m "fix(net): close M-01 HTTP/1.0 framing for v0.9.4"

# 4. Annotated tag on the release commit (already exists; do not recreate).
git tag -a v0.9.4 -m "EQProxy v0.9.4" 24e6dc6b29728b8ce16c197642a58268a27d89bd
git cat-file -p v0.9.4

# 5. Push the branch, then the tag. The tag push triggers release.yml.
git push origin release/v0.9.4
git push origin v0.9.4

# 6. The image is built, scanned and promoted by release.yml on the tag.
#    Manual equivalent only if reproducing the image job:
docker buildx build --file docker/Dockerfile \
  --platform linux/amd64,linux/arm64 \
  --tag ghcr.io/eqproxy/eqproxy:0.9.4 \
  --tag ghcr.io/eqproxy/eqproxy:latest \
  --provenance=mode=max --sbom=true --push .

# 7. Post-publication checks.
gh release view v0.9.4 --json tagName,isDraft,isPrerelease,publishedAt,assets
docker buildx imagetools inspect ghcr.io/eqproxy/eqproxy:0.9.4
docker buildx imagetools inspect exyonq/eqproxy:0.9.4
```

The release body is extracted from `CHANGELOG.md` by the workflow, not typed by
hand. The workflow rejects a tag whose `v$VERSION` does not match
`Cargo.toml`, and rejects a version without a changelog section.

## 5. What was not executed in this turn

- No `git commit`, `git push`, `git tag`, tag move or delete.
- No image build or push.
- No GitHub Release creation or edit.
- No change to `main`, `release/v0.9.4` or any remote ref.

The remote tag, GitHub Release and both registry references were only
**inspected** read-only. They already existed before this turn; this runbook does
not claim credit for publishing them.

## 6. Remaining work for v0.9.4

None in the repository: the release commit, tag, binaries, source tarball,
checksums, SBOMs, Scout reports and images are consistent. The open findings in
`docs/audit-2026-11-09/` belong to the next patch release (v0.9.5), not to this
one.
