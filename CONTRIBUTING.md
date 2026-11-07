# Contributing to EQProxy

EQProxy is a reverse proxy and load balancer in Rust. It is pre-1.0: v0.9.2 is
the latest published release (2026-11-06). Issues and pull requests are
welcome; for a change that touches behaviour, configuration or the wire
protocol, open an issue first so the scope can be agreed before code is
written.

## Development setup

Install a Rust toolchain: the workspace uses edition 2024, so **Rust 1.85 or
newer** is required. The version every leg actually builds with is pinned in
`rust-toolchain.toml` (currently 1.99.1) and `rustup` installs it
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

### Latest published release — v0.9.2

`v0.9.2` was published `2026-11-06T23:11:38Z` from the annotated tag `v0.9.2`
(`1ba58ea21388a5a69f8a3d577576ae9e14ec312b`) over commit
`6478f10b2d92792f9dba1c0cec5753fb738d9e89` (`release: publish Linux-only
v0.9.2 artifacts`). The tag run
[42281241472](https://github.com/EQProxy/EQProxy/actions/runs/42281241472)
finished `success` with all five jobs green, and its `release` job produced the
ten assets: two Linux binaries, the source tarball, four CycloneDX SBOMs, the
two Scout reports and `SHA256SUMS.txt`. Its image job built and scanned the
`0.9.2-candidate` index and promoted that exact digest.

| Release asset | SHA-256 |
|---|---|
| `eqproxy-linux-amd64` | `e12aa71290ab6a9c9b69f585f3f765a445e2130790bfa72149668c0a45b59929` |
| `eqproxy-linux-arm64` | `7946fa089b6d8c74590763d913e46e9ad183002b55a61425eb87ec8b103bc29e` |
| `eqproxy-0.9.2-src.tar.gz` | `a466d37098e203be1819b142c68fb7e78bb9e955b0bc545920b7630f64bf7cdd` |
| `SHA256SUMS.txt` (the file itself) | `998bf36883f7a31bb042608e38fd8b0ebdf958170b9f9c4807a42d4e559852ec` |

Those three lines are exactly what `SHA256SUMS.txt` checks; the SBOMs and the
Scout reports are not listed in it.

Both registries serve one OCI index, promoted by digest after the Scout scan:
`sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f`
for `ghcr.io/eqproxy/eqproxy:0.9.2`, `ghcr.io/eqproxy/eqproxy:latest`,
`docker.io/exyonq/eqproxy:0.9.2` and `docker.io/exyonq/eqproxy:latest`. The
v0.9.2 versions are copied to the `EQProxy` account by the migration; the
digest does not change. The index's platform manifests are
`sha256:b984b522fa32b82830194183a52a8238f34a5dc95545a29aff8d574ede7084e6`
(`linux/amd64`) and
`sha256:d8b681d54304202ce24149390a05140f61448cf5a68c32a56a358437525f29ba`
(`linux/arm64`); Docker Scout reports **0 critical / 0 high** on both, and
`docker run … --version` on the published tag answers `eqproxy 0.9.2`. There
is no Darwin asset: the release is Linux-only. The full record — local gates,
all ten asset digests, the local preparation builds that were **not** shipped,
and the audit legs — is [docs/release-0.9.2.md](docs/release-0.9.2.md).

Only a `v*` tag publishes. The release workflow needs two repository
secrets (Settings → Secrets and variables → Actions); neither belongs in the
tree:

| Secret | Value |
|---|---|
| `DOCKERHUB_USERNAME` | Docker Hub account that owns `exyonq/eqproxy`. |
| `DOCKERHUB_TOKEN` | Docker Hub token with read/write on that repository. |

GHCR needs no secret: the image job pushes to `ghcr.io/eqproxy/eqproxy`
with the workflow's `GITHUB_TOKEN` and `packages: write`, because the package
belongs to the same account as the repository and is linked to it. The former
`GHCR_TOKEN` PAT is retired.

The image job is a hard gate: it builds one candidate index, scans both
`linux/amd64` and `linux/arm64` with Docker Scout
(`--only-severity critical,high --exit-code`) and only promotes that exact
digest. The release job requires both Scout reports, so a critical/high
finding or a missing report fails the tag. Never bypass, waive or replace it
with a smoke check.

The package migration to `EQProxy` (see
[docs/package-transfer-eqproxy.md](docs/package-transfer-eqproxy.md)) copies
the v0.9.2 and `latest` tags to `ghcr.io/eqproxy/eqproxy` without changing
their digests. GitHub does not transfer GHCR packages between owners and does
not redirect the old namespace, so the old package is retired only after a
deprecation window. Because the package and the repository belong to the same
account and the package is linked to the repository, the workflow
authenticates with its own `GITHUB_TOKEN` under `packages: write`; no PAT is
involved. GitHub Packages
only accepts **classic** PATs, which is one more reason the long-lived token
is gone. Delete the `GHCR_TOKEN` secret and revoke the PAT after the first
successful release on the new namespace.

Docker Hub still authenticates with `DOCKERHUB_USERNAME`/`DOCKERHUB_TOKEN`;
rotate the Hub token the same way, update the secret and re-run the failed tag.

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
