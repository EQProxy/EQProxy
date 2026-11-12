# v0.9.5 — release preparation evidence

Date: 2026-11-11. Branch `release/v0.9.5`, base `1a7bf12`. Tag `v0.9.5` is
annotated and points at the release commit that contains this file; the commit
is the one recorded in the final delivery and in Hindsight.

Binary: `target/release/eqproxy`, `eqproxy 0.9.5`,
SHA-256 `29abadc1ac0f43975544b42a77f19fec458e644e41136492e1efa0955fd3904d`.

## M-01

M-01 was already fixed in `24e6dc6` (`fix(net): close M-01 HTTP/1.0 framing for
v0.9.4`) and `v0.9.4` is an ancestor of this release tree. The final tree
re-runs its regressions:

- `cargo test -p eqproxy-net --lib chunked_transfer_encoding_is_rejected_in_http10`
  → **1 passed, 0 failed**.
- `cargo test -p eqproxy-net --test e2e_http1 http10_` → **2 passed, 0 failed**:
  `http10_client_gets_dechunked_close_delimited_response` and
  `http10_head_response_never_advertises_chunked_framing`.

M-01 is not an open blocker for v0.9.5.

## Gates on the release tree

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace --locked` | exit 0 — 799 passed, 0 failed, 2 ignored across 39 result blocks |
| `cargo build --workspace --release` | exit 0 |
| `git diff --check` | exit 0 |
| `cargo check --manifest-path fuzz/Cargo.toml --bins` | exit 0 |

The 39 result blocks are 36 test binaries plus 3 doctest blocks; the README's
"36 targets" is the binary count. The suite log is `/tmp/v095-suite-final.log`;
the gate logs are `/tmp/v095-final-fmt.log`, `/tmp/v095-final-clippy.log`,
`/tmp/v095-final-release_build.log` and `/tmp/v095-final-suite.log`.

## Release preparation

- `Cargo.toml` workspace version and the internal `eqproxy-*` pins are `0.9.5`;
  `Cargo.lock` and `fuzz/Cargo.lock` carry the same versions.
- `CHANGELOG.md` has `## [v0.9.5] — 2026-11-11` with AUD-01, AUD-02, ADV-01,
  ADV-02, AUD-03, the checkout liveness fix and the `rules` fuzz-target fix.
- `README.md` and the `docker/Dockerfile` example name v0.9.5.
- The tree has `v0.9.3` and `v0.9.4` as ancestors:
  `git merge-base --is-ancestor v0.9.3 HEAD` and
  `git merge-base --is-ancestor v0.9.4 HEAD` both exit 0.
- Tag/version and changelog extraction are checked with the release workflow's
  own shell/Python snippets; the result is recorded in the final delivery.

## Limits

No push, tag publication or workflow dispatch was performed. Docker Scout,
cross-architecture image E2E and the remote release jobs remain the user's
publication step; they are not claimed here.

**2026-11-12 update.** Publication was executed after this record was
written: the tag was set to `3451c19` (this tree plus the Wolfi base refresh
in `docker/Dockerfile`) and the GitHub release `v0.9.5` was published
(run `43232479817`). The paragraph above describes the state on 2026-11-11 and
is kept as run; the published-artifact facts and the checks executed against
them are in [the v0.9.5 release record](../release-0.9.5.md).
