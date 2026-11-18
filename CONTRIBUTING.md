# Contributing to EQProxy

EQProxy is a reverse proxy and load balancer in Rust. It is pre-1.0: v0.10.0 is
the latest published release (2026-11-17). Issues and pull requests are
welcome; for a change that touches behaviour, configuration or the wire
protocol, open an issue first so the scope can be agreed before code is
written.

## Development setup

Install a Rust toolchain: the workspace uses edition 2024, so **Rust 1.85 or
newer** is required. The version every leg actually builds with is pinned in
`rust-toolchain.toml` (currently 1.100.0) and `rustup` installs it
automatically when you enter the checkout; CI and the release image use that
same compiler, so bumping the pin is a deliberate change with its own
verification rather than a moving `stable`.

```bash
git clone https://github.com/EQProxy/EQProxy.git
cd EQProxy
git config core.hooksPath .githooks
cargo build --workspace
cargo test --workspace
```

Debug and test builds generate the self-signed TLS/mTLS fixtures with the
`openssl` CLI into `OUT_DIR/fixtures`; keep `openssl` on `PATH` (the CI
runners and macOS provide it). Release builds skip generation, so the release
binary and image neither need OpenSSL nor ship test keys. The generated PEMs
are never committed.

Run the proxy against the example configuration, which is also the quickest
smoke test of a change:

```bash
cargo run -p eqproxy-cli -- config validate --config eqproxy.toml.example
cargo run -p eqproxy-cli -- run --config eqproxy.toml.example
```

`config validate` is offline and binds nothing; `run` serves with the example
listeners until `SIGINT`.

The optional CodeMap checks read a local index under `.codemap/`:

```bash
codemap index --root .          # refreshes .codemap/index.db and .codemap/pages/
codemap lint --severity warning
```

`.codemap/pages/*.md` (`crates`, `gaps`, `ipc-map`, `ui`) are generated
projections of that index, not hand-written documents, and are not committed.
`codemap index` renders them; `codemap pages export` only copies pages that
already exist. Regenerate them with `codemap index` after reindexing, or
`codemap lint` reports `stale_page` for every page.

## Before opening a PR

Run the build gates CI runs on every push, with the same flags:

```bash
python3 .github/scripts/check-secrets.py --self-test
python3 .github/scripts/check-secrets.py
python3 .github/scripts/check-secrets.py --range origin/main..HEAD
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The secret gate rejects private keys with no allowlist; public certificates
are not matches. It scans the tracked tree, staged blobs (`--staged`, also run
by the `.githooks/pre-commit` hook after `core.hooksPath` is configured) and
every commit added by a push or pull request (`--range`). The range mode scans
added lines, so a key added and then deleted in the same change is still
caught. The release workflow repeats these checks and scans the exported
source tarball for private keys.

CI also runs the admin-socket ACL journey on native Linux runners
(`.github/scripts/admin-unix-acl-e2e.sh`, amd64 and arm64). It needs root, `setpriv`
and POSIX ACLs, so it is not part of the loop above; run it on Linux before a
change that touches the admin plane:

```bash
cargo build --release --locked -p eqproxy-cli
sudo env EQPROXY_BIN="$PWD/target/release/eqproxy" \
  bash .github/scripts/admin-unix-acl-e2e.sh
```

Then exercise the behaviour you changed the way a user would: start the
binary with a configuration that reaches your code path and check the
observable result (status, body, headers, log line, metric, clean shutdown).
A green build or a process that starts is not evidence that a feature works.
Request a review before merge; do not assume an automatic review.

## Tests and evidence

- Every fix or feature arrives with the smallest test that demonstrates the
  contract or observable effect. Name the level honestly: unit, contract,
  integration or E2E.
- When a change crosses sockets, TLS, HTTP/2 or QUIC frames, the test uses the
  real dependency (a real origin, a real client) — a mock presented as
  integration is not accepted.
- Cover the error and boundary cases, and state what must happen when a
  dependency fails or invalid input arrives.
- Never weaken an assertion, skip a test or change an expected result to make
  a check pass. If a test has to change, explain why in the PR.
- Report checks that could not run as **not executed**, never as a pass.

## Audits

A change is not finished until it has been audited on four independent legs.
Each one answers a different question, and none of them substitutes another:

| Leg | Question | Tool |
|---|---|---|
| Toolchain | Does the workspace build, lint and pass the whole suite? | `veyra audit . --profile standard` |
| Static | Are there stubs, fake tests, swallowed errors, panic/`unsafe` misuse or invented dependencies? | Rust Audit `scan` |
| Structure | Is the change where it says it is, and what else does it reach? | CodeGraph Rust (`codegraph-rust`) |
| Memory | What must the next session know? | Hindsight note |

The toolchain and static legs are blocking: a rejected verdict or a scan
finding is diagnosed, never silenced. CodeGraph is evidence of structure only
— its graph citations are checked against the source and its prose is
advisory, so it never counts as proof that something works. EvoForge is no
longer part of this process (removed 2026-10-24); the slice eval it ran could
not say anything about the real programme that the suite had not already
said.

Run the Rust Audit gate through
[`.github/scripts/rust-audit-gate.sh`](.github/scripts/rust-audit-gate.sh). It
takes no scan arguments and always applies the versioned reviewed
`rust-audit-baseline.json` with `--fail-on high`; do not run `rust_audit.py
scan` directly for the gate and do not pass `--baseline` by hand. The gate
works in a clean clone and in CI. It also checks occurrence counts, so a
second high with the same reviewed fingerprint cannot hide behind the tool's
set-based diff; a new, extra or stale entry exits `1`. A missing or malformed
baseline, a tool/engine mismatch or a scan execution failure exits `2` and is
**NOT EXECUTED / BLOCKED**, not a pass. Never create an empty baseline. See the
[Rust Audit runbook](docs/rust-audit-runbook.md).

## Scope discipline

- [`SPEC.md`](SPEC.md) is the product contract. Behaviour that is not in the
  spec needs a spec change first.
- Design decisions live in [`docs/adr/`](docs/adr/): a change to a module
  interface, the configuration format, the wire behaviour or a dependency
  needs a new or amended ADR.
- All code is safe Rust: `unsafe_code = "forbid"` is set at the workspace
  level and there are no exceptions. `unsafe` and FFI may only come from
  dependencies.
- New dependencies are justified in an ADR, pinned in `Cargo.lock`, audited
  with `cargo deny` and must not pull in a second crypto provider or a C
  toolchain for compression.
- Version numbers are pinned: no unpinned "latest" enters `Cargo.lock`.
- HTTP/3, gRPC and WebSockets over h2 are on `main` but stay labelled
  experimental until their evidence says otherwise; do not present them as
  production-verified beyond what `docs/evidence/` documents.

## What never enters a commit

Credentials, private notes and generated artifacts stay out of the diff. Test
TLS material is generated at build time: do not add `.pem` or `.key` files to
the tree. Imported conversation or tool attachments (for example
`.cloud-attachments/`) are local-only and belong in `.git/info/exclude`, not
in a commit or the Docker context.

Enable **GitHub secret scanning and push protection** in
**Settings → Code security** for the repository. CI and the pre-commit hook
are the local backstops; push protection is the remote gate that refuses a
credential before it becomes reachable on GitHub.

Before pushing, this check must print nothing:

```bash
# No AI/bot co-authorship trailers: the owner commits as a human author.
git log origin/main..HEAD --format=%b | grep -iE '^co-authored-by:'
```

## Releases

### Latest published release — v0.10.0

`v0.10.0` was published `2026-11-17T21:47:23Z` from the annotated, SSH-signed
tag `v0.10.0` (`fb56e28…`, object) over commit `bdf88bb`. The tag run
[44183459466](https://github.com/EQProxy/EQProxy/actions/runs/44183459466)
finished `success`, after two earlier tag runs failed and each produced one of
the commits the tag points at: `6de908c` repins the Wolfi base for
`CVE-2026-84243`, and `bdf88bb` drops a semver-contract exception that no
longer matched. Its `release` job produced the ten assets — two Linux
binaries, the source tarball, four CycloneDX SBOMs, the two Scout reports and
`SHA256SUMS.txt` — and its image job built and scanned the
`0.10.0-candidate` index and promoted that exact digest.

| Release asset | SHA-256 |
|---|---|
| `eqproxy-linux-amd64` | `f59b512e4f44a93f26d914ff613206c8c3259ac3ec2ad1dec1e2d087c409c658` |
| `eqproxy-linux-arm64` | `ab02916787b11e6d63745ec42167950f4772a0dd9236ea34ce2da29f11d96bce` |
| `eqproxy-0.10.0-src.tar.gz` | `46b07a1b6cf85cd4c2a57f3ea0b9523d0e57d384a29a25d8325841a050711775` |
| `eqproxy-core.cdx.json` | `3abd2e610aabaad07fe2f447fb39af37bdeaba5e4638420163dbe4628e49eca8` |
| `eqproxy-config.cdx.json` | `2f2460d6d0d734b449c6d8957a663fe45bffddc761aeaddfcffb20c50235cf12` |
| `eqproxy-net.cdx.json` | `d222de4bfd77e35aee38a59f0d9d39aad1fddf3e7962d304797d57aaf1178e0f` |
| `eqproxy-cli.cdx.json` | `45736cf1697e032e1ce9c48f5b5bef2766c5c6899c047cf09d33ecb1eeaa3159` |
| `SCOUT-0.10.0-linux-amd64.md` | `566b60307ca616eb80b164929552cbc73342f919ec1d0ef992e602d156eca093` |
| `SCOUT-0.10.0-linux-arm64.md` | `dc0bfff8432e45332a20185df8b51dc3d9add1b6bbfc067c89d1acff72a44259` |

Those nine lines are exactly what `SHA256SUMS.txt` checks in this release —
`shasum -a 256 -c SHA256SUMS.txt` reads `9/9 OK` from this checkout — and the
checksum file itself, being an attached asset, is not listed in it. From
v0.10.0 the file covers every attached asset: the `release` job assembles a
flat `dist/` (both binaries, the source tarball, one SBOM per workspace member
and both Scout reports) and its checksum step refuses to continue unless
`SHA256SUMS.txt` holds one line per published asset —
`SHA256SUMS.txt covers 9 of 10 published assets` is the failure a stray or
unhashed file produces. A missing crate SBOM or a missing architecture report
fails the release before it is created, so a single `sha256sum -c` verifies the
whole download. The two Linux lines keep their original form at the top of the
file, which is why the v0.6.x instructions still work unchanged.

Both registries serve one OCI index, promoted by digest after the Scout scan:
`sha256:f8eee767f805c085bd624d847b6844aaa8d7436741354d9110b05d1a2013aea5`
for `ghcr.io/eqproxy/eqproxy:0.10.0`, `ghcr.io/eqproxy/eqproxy:latest`,
`exyonq/eqproxy:0.10.0` and `exyonq/eqproxy:latest`. Its platform manifests are
`sha256:a3f30263…` (`linux/amd64`) and `sha256:356e47f5…` (`linux/arm64`),
each reported **0 critical / 0 high** by Docker Scout. The release workflow
re-pulled the published tags and replayed the journeys (`--version`,
configuration and live proxied traffic) on both architectures before finishing
`success`. There is no Darwin asset: the release is Linux-only.

Only a `v*` tag publishes. The release workflow needs two repository secrets
(Settings → Secrets and variables → Actions); neither belongs in the tree:

| Secret | Value |
|---|---|
| `DOCKERHUB_USERNAME` | Docker Hub account that owns `exyonq/eqproxy`. |
| `DOCKERHUB_TOKEN` | Docker Hub token with read/write on that repository. |

The GHCR push uses the workflow's automatic `GITHUB_TOKEN` with
`packages: write`; `ghcr.io/eqproxy/eqproxy` is linked to
[`EQProxy/EQProxy`](https://github.com/EQProxy/EQProxy). No package PAT secret
is required.

The image job is a hard gate: it builds one candidate index, scans both
`linux/amd64` and `linux/arm64` with Docker Scout
(`--only-severity critical,high --exit-code`) and only promotes that exact
digest. The release job requires both Scout reports, so a critical/high
finding or a missing report fails the tag. Never bypass, waive or replace it
with a smoke check.

The GHCR package lives in the same user namespace as the repository owner, so
the image job pushes with the workflow's `GITHUB_TOKEN`; no package PAT or
classic-token exception is needed.

To rotate the Docker Hub token, create a new token on the `exyonq` account
with write access to `exyonq/eqproxy`, update the `DOCKERHUB_TOKEN` secret, and
re-run the failed tag. Revoke the old token afterwards.

## Commit style

Subject in the imperative, prefixed with the area it touches, body explaining
why:

```
feat(net): close idle backend h2 sessions and replay refused streams

docs: record the backend HTTP/2 session lifecycle slice
```

Use `feat`, `fix`, `docs`, `ci`, `test` or `refactor` as the prefix and the
crate or area in the scope. Do not add AI co-authorship footers or
"Generated with …" attribution: the commit is signed by its human author. The
owner's human identities (`Doken-Tokuyama`, `ExyonQ`) are both valid; only an
assistant or bot as co-author is refused.

## Documentation and changelog

- Documentation is published in **English only**.
- Public docs live under `docs/` (ADRs, migration guides, differential
  corpus) and the top-level `SPEC.md`, `THREAT_MODEL.md` and
  `BENCHMARK_PLAN.md`.
- Every user-visible change gets a `CHANGELOG.md` entry under `[Unreleased]`,
  in the Keep-a-Changelog style the file already uses, naming what was
  verified and linking its evidence.
- Evidence goes under `docs/evidence/` with the commands, the artifact under
  test (binary or image digest) and the raw data. A conclusion without raw
  data is not evidence.
- A test cited in `docs/evidence/` must exist in the Rust tree of the same
  commit. CI and the release pipeline run
  `.github/scripts/check-evidence-test-citations.py`, which refuses a
  test-shaped name the tree does not define; a name kept on purpose as
  history is declared with its reason in
  `.github/scripts/evidence-test-citations-allow.txt` (run the script locally
  to see the verdict).
- Two blocks of the Linux suite only hold for an **unprivileged** uid: each
  asserts that a `chown(2)` to a foreign group is refused. Run as root they
  print `SKIP (running as root): <reason>` and carry on. CI runs the suite
  both ways: the unprivileged run must print none of those lines, and the root
  run must print exactly the ones declared in
  `.github/scripts/uid-skips.txt`. Run
  `python3 .github/scripts/check-uid-skips.py --self-test`, or give it a
  capture with `--root-log <log>` / `--user-log <log>`; a root capture needs
  `--nocapture`, or the marker never reaches the log.
- `docs/` publishes documentation (`.md`) and the recorded data only, never
  code. The harnesses that produced the measurements live outside the
  repository, in the maintainer's tree, and the evidence names them so a
  reader knows which program produced each number and can ask for it.

## Benchmarks

Scenario IDs and methodology come from [`BENCHMARK_PLAN.md`](BENCHMARK_PLAN.md)
§3–§5: same backends and topology for both products, at least five
repetitions, medians with the full range, and every scenario published —
including the ones EQProxy loses. Never cherry-pick the best run, and never
compare numbers taken on a host whose load you did not record.

## License

EQProxy is dual-licensed under the **MIT** and **Apache-2.0** licenses
(`LICENSE-MIT`, `LICENSE-APACHE`). By submitting a contribution you agree
that it is licensed under both, with no additional terms.

## Security

Do not open a public issue with a working exploit. Describe the affected
surface and the impact, and ask the maintainers for a private channel; the
threat model in [`THREAT_MODEL.md`](THREAT_MODEL.md) is the reference for what
is in scope.
