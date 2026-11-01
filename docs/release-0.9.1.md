# v0.9.1 — publication checklist and artifact gate

Language: **English**.

**What this page answers:** what must be true before the `v0.9.1` tag is
pushed, what the pipeline does with it, and how the published artifacts are
verified afterwards. This is the procedure for the 0.9.1 cut; it is **not a
claim that the publication has happened**. The cut commit exists
(`ff89879ce24c9554a0d7b6862b22d24dc5b500c5`, 2026-11-01) and the local source
gates and all four audit legs were run on it; the results are recorded in §2.1
and §2.2. No tag exists, no image has been promoted, and every gate below that
is marked **NOT EXECUTED** has not run.

- Version: **0.9.1** (`Cargo.toml` `[workspace.package] version`).
- Release notes: [CHANGELOG.md](../CHANGELOG.md) section
  `## [v0.9.1] — 2026-11-01`, extracted automatically by the `release` job.
- Pipeline: [.github/workflows/release.yml](../.github/workflows/release.yml).

## 1. The release commit

The release commit contains the complete 0.9.1 tree, not only the version
bump: the new code, tests, ADRs and evidence are part of it, not a later
follow-up commit. The version-only part is:

| File | Change | Why |
|---|---|---|
| `Cargo.toml` | `version = "0.9.1"` in `[workspace.package]` | single source of truth: tag gate, image tag, release notes and SBOM |
| `eqproxy-config/Cargo.toml`, `eqproxy-net/Cargo.toml`, `eqproxy-cli/Cargo.toml` | the `eqproxy-*` path dependencies pinned at `0.9.1` | a stale pin fails to resolve against the workspace version |
| `Cargo.lock` | the four `eqproxy*` entries at `0.9.1` | the release builds with `--locked` |
| `fuzz/Cargo.lock` | the four `eqproxy*` entries at `0.9.1` | the fuzz workspace has its own lockfile and is not built by CI |
| `CHANGELOG.md` | `## [Unreleased] — v0.9.1` becomes `## [v0.9.1] — 2026-11-01` | the release job refuses to publish without a section for the version |

Regenerate the lockfiles with `cargo metadata`, never by hand, and inspect the
diff because a lockfile that was already behind its manifest carries that
catch-up too:

```console
$ cargo metadata --format-version 1 > /dev/null
$ cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null
$ grep -A1 '^name = "eqproxy-core"' Cargo.lock
name = "eqproxy-core"
version = "0.9.1"
```

The release notes section records the user-visible changes that were still
missing — the unconditional `Server: eqproxy` banner and the `Date` header on
responses the proxy generates. The release commit also
contains the English translation of [docs/semantics.md](semantics.md), the
corrected T13/T14 wording in [THREAT_MODEL.md](../THREAT_MODEL.md), and the
repair of the ADR set's relative links (the `SPEC.md` targets, two references
to a non-existent ADR-0011, and two roadmap links that were dead in a clean
clone); all of them are documentation fixes rather than runtime changes.

## 2. Local gates before the tag

Every one of these must be green **on the cut commit**. None is replaced by a
green run on an earlier working tree.

```console
$ cargo fmt --all -- --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace
$ cargo build --workspace --release
$ python3 .github/scripts/check-evidence-test-citations.py --self-test
$ python3 .github/scripts/check-evidence-test-citations.py
$ python3 .github/scripts/check-short-health-probes.py --self-test
$ python3 .github/scripts/check-short-health-probes.py
$ python3 .github/scripts/check-uid-skips.py --self-test
```

The `uid-skips` verdict reads a transcript, not the source: it runs on the two
suite runs the `guard` job performs (`--nocapture`, unprivileged then as
`root`, `--user-log` / `--root-log`), so the local result is only its
`--self-test` until those two logs exist.

Version invariants the pipeline enforces, checkable before pushing:

```console
$ sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1        # 0.9.1
$ sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml          # 1.99.1
$ grep '^ARG RUST_VERSION' docker/Dockerfile                       # 1.99.1, must match
$ grep -n '^## \[v0.9.1\]' CHANGELOG.md                            # the release-notes section
```

`ci.yml` fails on a toolchain/`ARG RUST_VERSION` mismatch, and the `guard` job
fails on `v$VER != $GITHUB_REF_NAME`.

### 2.1 Cut-commit result (`ff89879`, 2026-11-01)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit **0** (Veyra `rustfmt`) |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit **0** (Veyra `clippy`) |
| `cargo test --workspace` | exit **0** — **36 targets, 715 passed, 0 failed, 2 ignored** (Veyra `cargo-test` logs); `cargo-nextest` on the same commit: 715 run, **715 passed**, 2 skipped |
| `cargo build --workspace --release` | exit **0**; `target/release/eqproxy --version` → `eqproxy 0.9.1` |
| `python3 .github/scripts/check-evidence-test-citations.py` | exit **0** (101 documents, 229 cited identifiers, 700 `#[test]` functions) |
| `python3 .github/scripts/check-short-health-probes.py` | exit **0** (18 probes found, 2 declared); `--self-test` exit **0** |
| `python3 .github/scripts/check-uid-skips.py --self-test` | exit **0** |
| Local Markdown link check over the release tree's Markdown (165 files) | **1,145 links, 0 broken, 0 local-only**, 11 anchors, exit **0** (the cut commit itself checked 1,144; §2.2's triage link is the one this page adds) |
| CHANGELOG extraction regex for `0.9.1` | section found at line 6, date `2026-11-01` |

The link sweep's method, repair breakdown and limits are recorded in
[v091-link-sweep.md](evidence/v091-link-sweep.md). It was re-run on the
commit, not on the working tree that preceded it.

Every row was produced on commit `ff89879ce24c9554a0d7b6862b22d24dc5b500c5`
with a clean working tree (`git status --porcelain` empty), except the link
row's 1,145: that is the re-run after this page's audit table added its one
link, on a tree whose only change is documentation. The
`root`/unprivileged `--nocapture` pair and the two `uid-skips` verdicts remain
CI-only until the tag run produces those transcripts.

### 2.2 Audits on the cut commit

| Leg | Result |
|---|---|
| Veyra Proof | **ACCEPTED**, exit **0** on `ff89879` (`20261101T070732Z`): rustfmt, check, clippy, cargo test, nextest, llvm-cov, audit, deny, machete, veyra-e2e and anti-theater OK. `cargo-geiger` INCONCLUSIVE (coverage warnings in 4 runs, reported and not counted as a pass); dylint and semver-checks skipped with their reasons. Evidence: `.veyra/evidence/20261101T070732Z/report.md` (local-only). A duplicate launch overlapped this leg once; that contended run (`20261101T065643Z`) was **REJECTED** because its E2E binary was SIGKILLed mid-scenario. It stays on disk, and the accepted rerun above is the one cited. |
| Rust Audit | exit **1** by rule: **0 critical / 7 high / 417 medium / 111 low**. The seven high are the `cache.rs`/fuzz set triaged in [v085-rust-audit-triage](evidence/v085-rust-audit-triage.md); no new high on the commit. Evidence: `report/rust-audit-20261101-075704.md` and `report/not-executed-2026-11-01.md` (local-only). |
| CodeGraph Rust | index exit **0** over `ff89879`: 84 files, 81,576 lines, 8,940 nodes, 15,784 edges, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms 993 238`. `agent impact` on `current_http_date` reports 9 direct dependents; source-checked: `http1.rs:1117/1141/1241/1288/1329`, `http2.rs:336`, `http3.rs:953`, `ws.rs:321` and `grpc.rs:647` (the agent's ranges are a few lines off; the set is right). `agent context` locates the pages in `error_pages.rs:49-83` and the writers in `http1.rs`; it flags `get_reverse_dependencies` returning nothing, so the grep above is the authority. Stats: `/tmp/cg-index-v091.json`. |
| Hindsight | **Stored** for this cut, bank `eqproxy`: first id `6a6eb5dc-30f2-4096-b065-86166016a866` (the CLI returned a list of memory ids for the note). |

All four legs ran on the cut commit, not on an earlier working tree. A
docs-only commit on top of it records these results and updates the Date
evidence page; the tagged tree therefore differs from the audited one only in
those Markdown files.

### 2.3 `Date` on locally generated responses

The gap recorded before the cut is **closed in this tree**: every response the
dataplane assembles itself — local errors, custom error pages and `errorloc`,
redirects and direct responses, auth denials, h2/h3 and WebSocket refusals and
gRPC local answers — now carries one IMF-fixdate `Date` (RFC 9110 §6.6.1).
Proxied and cached responses keep the origin's value and are never given a
second one; a rule that sets `Date` cannot forge it. The formatter is
`eqproxy-net/src/date.rs`, with five unit tests, and the real-socket suites
assert the header (h1, h2, h3, gRPC, error pages, local rules); evidence in
[v091-date-header](evidence/v091-date-header.md).

Deliberate limits of the change: an origin that omits `Date` is relayed as-is
(the proxy is not the generator of a proxied response), and
`error_pages` `format = "raw"` still keeps only status, `Content-Type` and
body, so the generated `Date` replaces any written in the raw file. The
`100 Continue` interim response keeps no header fields, as before.

### Image and Scout gate

The `image` job builds the candidate index for **linux/amd64 and linux/arm64**,
scans each architecture with Docker Scout (`--only-severity critical,high
--exit-code`), exercises the image and promotes only the scanned digest. The
same gate must be reproducible locally before the tag:

```console
$ docker buildx build --pull --file docker/Dockerfile --platform linux/amd64 \
    --provenance=mode=max --sbom=true --tag eqproxy-release:0.9.1-amd64 --load .
$ docker buildx build --pull --file docker/Dockerfile --platform linux/arm64 \
    --provenance=mode=max --sbom=true --tag eqproxy-release:0.9.1-arm64 --load .
$ docker scout cves --only-severity critical,high --exit-code \
    --platform linux/amd64 eqproxy-release:0.9.1-amd64
$ docker scout cves --only-severity critical,high --exit-code \
    --platform linux/arm64 eqproxy-release:0.9.1-arm64
```

The container journey and the Scout gate on this 0.9.1 tree are **NOT
EXECUTED**: the amd64 build started on 2026-11-01 was stopped by the owner
before it completed; no amd64 image, no journey transcript and no Scout report
exist for it. The previous arm64 image (`eqproxy:v091-arm64`) and its TLS
journey are evidence for that architecture only and are not a substitute for
the amd64 run or for the candidate index.

## 3. The commit itself

The commit must not carry material the repository does not publish, and it
must not carry AI or bot attribution ([CONTRIBUTING.md](../CONTRIBUTING.md)
authorship policy). Before pushing the tag:

```console
$ git log <base>..HEAD --format=%b | grep -iE '^co-authored-by:'
$ git log <base>..HEAD --format=%s,%b | grep -iE 'generated with|co-authored-by'
```

Both must return nothing. Inspect the outgoing diff for maintainer-local files
as well; a build context or an archive that absorbed one is not fixed by a
later commit. The source tarball is exported with `git archive`, so untracked
local material does not enter it by construction; verify the file list anyway.

The cut commit `ff89879` was checked locally against both criteria and against
the maintainer-local path list: no path in `git show --name-only HEAD` matched
a local-only entry, `git log -1 --format=%b` carried no attribution trailer and
`git status --porcelain` was empty.

## 4. Tag and push

```console
$ git tag -a v0.9.1 -F release-notes-v0.9.1.md      # the notes of the CHANGELOG section
$ git push origin v0.9.1
```

A `v*` tag runs the whole pipeline; a manual `workflow_dispatch` runs it as a
dry run where release creation and registry pushes stay off.

| Job | What it proves |
|---|---|
| `guard` | both suite tiers on the tag commit (`--nocapture`, unprivileged and `root`), the three gate self-tests and verdicts, and `v$VERSION == $GITHUB_REF_NAME` |
| `darwin`, `linux` | the binaries build with the pinned toolchain and pass their E2E journeys (real CLI, live proxied traffic) before any checksum exists |
| `image` | builds and pushes the `0.9.1-candidate` index, records its digest, runs **Docker Scout on both architectures with `--exit-code` and `--only-severity critical,high`**, replays the version/config/live-traffic and admin-ACL journeys inside the image, then promotes the *scanned digest* to `:0.9.1` and `:latest` (GHCR) and mirrors it to Docker Hub |
| `release` | assembles `dist/` only after both builds verified, exports and checks the source tarball of the released commit, writes and re-checks `SHA256SUMS.txt`, and creates the GitHub release with the notes, SBOM and Scout reports |

Nothing is published by a job whose verification failed: the registry push and
the release creation are gated on the earlier jobs, and the promotion copies a
digest that was scanned, not a rebuild.

## 5. After publication — the artifact gate

This is the gate that closes the version. It runs after the tag run finishes;
until then every line is pending. Every line is a check with its expected
result.

```console
$ gh run list --workflow release.yml --limit 1        # the tag run: success, every job
$ sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1   # 0.9.1 == tag v0.9.1
$ gh release view v0.9.1                             # assets: 4 binaries, src tarball, SHA256SUMS, SBOM, 2 Scout reports
$ gh release download v0.9.1 --pattern 'eqproxy-*' --dir /tmp/rel && cd /tmp/rel && \
    (command -v sha256sum >/dev/null && sha256sum -c SHA256SUMS || shasum -a 256 -c SHA256SUMS)
$ docker pull ghcr.io/exyonq/eqproxy:0.9.1 && docker run --rm ghcr.io/exyonq/eqproxy:0.9.1 --version   # eqproxy 0.9.1
$ docker scout cves --only-severity critical,high --exit-code ghcr.io/exyonq/eqproxy:0.9.1   # 0C 0H, exit 0
$ docker buildx imagetools inspect ghcr.io/exyonq/eqproxy:0.9.1 \
    --format '{{json .Manifest}}' | jq -r '.manifests[].platform.architecture'   # amd64 and arm64
$ docker buildx imagetools inspect ghcr.io/exyonq/eqproxy:0.9.1 \
    --format '{{.Manifest.Digest}}'                                              # == the scanned candidate digest
```

- The digest check is the one that matters: `:0.9.1`, `:latest` and the Docker
  Hub mirror must carry **exactly** the index digest the Scout gate scanned.
- Both architectures are re-pulled and replayed by the pipeline itself; repeat
  one journey by hand on the published tag (`--version`, `config validate` and
  one proxied request) to confirm the public artifact, not the local build.
- Then, in a follow-up commit, update what describes the published state:
  `README.md` (status line, install URLs, image tags), `repo.md` and
  `docs/docker-hub-readme.md`. They are not part of the cut commit on purpose:
  until the tag run succeeds they would claim a release that does not exist.
- Record the run id, the digests, the Scout reports and the re-pull transcript
  under `docs/evidence/`, so the version is reproducible from the repository
  alone.

## 6. If the tag run fails

- **A `guard`/build/E2E job failed:** nothing was published. Fix it in a new
  commit; the tag is not reused, because a second tag on the same version
  publishes artifacts that were never built from one commit. Bump the patch
  version instead (`0.9.2`) or, if the tag never left the machine, delete it
  before pushing.
- **The version pin or the notes extraction failed:** the mismatch is real
  (tag ≠ `Cargo.toml`, or no `## [v0.9.1]` section); fix the commit.
- **The image job failed before promotion:** the `-candidate` tag may exist but
  `:0.9.1` and `:latest` were not touched; the release job depends on the image
  job, so no release exists either.
- **The release job failed after the registry promotion:** the images are
  public and the GitHub release is missing. Do not re-tag: finish the release
  from the same commit with a re-run of the failed job, and verify the digests
  again before declaring the version published.
