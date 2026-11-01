# v0.8.5 — documentation truth pass

**Objective.** An external read-only review of v0.8.5 (2026-10-31, tree
`04859af`) reported that the dataplane holds up but that the architecture
documentation contradicts the shipped tree in several checkable places. This
increment verifies each documentation claim itself, repairs the ones that were
false, and records the rest as open. It changes no behaviour except one report
label.

Everything below was re-measured on the working tree at `04859af` plus this
pass; nothing is copied from the review.

## 1. Claims checked

| Review claim | Verdict | Evidence in this tree |
|---|---|---|
| `README`, `ADR-0004` and `THREAT_MODEL` say the stack is `hyper` | **Confirmed** | `grep -c hyper Cargo.lock` → 0; no manifest declares it; no hit in `eqproxy-*/src`. The HTTP/1.1 parser and engine are `eqproxy-net/src/http1.rs`; HTTP/2 uses `h2` |
| `ADR-0002` is described as pending but is Accepted | **Confirmed** | ADR-0002: `State: **Accepted** (2026-10-12)`; the format is implemented and versioned |
| `repo.md` is tracked and still describes v0.5.0 | **Confirmed** | `git ls-files repo.md` → tracked; status block said v0.5.0 and `config import` "planned, exits 2" |
| `THREAT_MODEL` is v0.1 and stale for shipped mitigations | **Confirmed** | v0.1 (2026-10-12); T11 still said PROXY reception "pending Phase 3" although `e2e_resilience.rs::proxy_receive_*` exercises it |
| "559 tests in 30 targets" | **Confirmed stale** | `cargo test --workspace` → **657 passed, 0 failed, 2 ignored** (659 cases), 31 test binaries, macOS arm64, 2026-10-31. At the `v0.8.5` tag (`6e938ae`) a worktree `cargo test --workspace -- --list` lists **654 cases in 31 targets**; the five extra cases are the uncommitted `Server: eqproxy` increment |
| Two evidence tables record `exit 0` with `--fail-on high` and 3 `high` | **Confirmed false** | A fresh scan exits **1** (see §3). `--fail-on high` fails on any `high` |
| Three monoliths | **Confirmed sizes** | `validate.rs` 3 918 lines (`check` at :38), `http1.rs` 3 971 (`serve_http` at :2232), `admin.rs` 2 933, `main.rs` 3 719. Not refactored here |
| `config import` counts "a revisar" differently from the report sections | **Confirmed** | `classify()` folds `Info` into `counts.review`; the header printed "a revisar N" while the detail splits `⚠️ revisar` and `ℹ️ informativo` |
| No `[profile.release]` | **Confirmed** | `Cargo.toml` has no `[profile.*]` |
| Fuzz workspace not in CI | **Confirmed** | `.github/workflows/ci.yml` has no fuzz job |
| Panel cookie has no `Secure` | **Confirmed** | `panel.rs::cookie_header`: `HttpOnly; SameSite=Strict; Path=/panel`; ADR-0029 documents those attributes, and the panel binds wherever `[admin]` binds (loopback by default) |
| Validation errors point at the section line, not the key | **Partially refuted** | An invalid value is pinned exactly: `mode = "https"` → `bad3.toml:8:8`, caret under the value, field, expected variants and an example. A **missing** field can only be anchored at the section header (`bad.toml:3:1`, `missing field backend`), because the key is absent |

Not re-run from the review: its manual E2E (502 to a dead backend, `config
import` 3/3/0), Docker, benchmarks and the remote CI run. They are neither
endorsed nor disputed here. `config validate` diagnostics were re-checked
(two files above): the claim needed the qualification in the table, not a fix.

## 2. What changed

- `README.md` — stack corrected (own HTTP/1.1 parser + `h2`, no hyper), ADR-0002
  stated as Accepted, suite figure replaced with the v0.8.5 measurement.
- `docs/adr/0036-own-http1-parser.md` — **new**: records the parser decision,
  supersedes ADR-0004's HTTP/1–2 row and explains why byte-exact framing and
  rejection of ambiguous messages stay in this project's code.
- `docs/adr/0004-network-stack-and-crate-layout.md` — state line marks the
  partial supersession; an "Implementation reality" note separates the Phase-0
  research from the shipped stack; the `hyper` mentions in the unsafe inventory
  and consequences are corrected.
- `THREAT_MODEL.md` — version 0.2, reviewed against the tree: T04, T07, T11,
  T13, T14, T15, T16 and T19 updated with code/test citations or an explicit
  partial/open status; `hyper` removed from the dependency `unsafe` list. Rows
  left as *Designed (scope)* (T05, T06, T08, T12, T17, T18, T20) genuinely have
  no shipped mitigation.
- `SPEC.md` — the v0.8 row's suite clause now cites the v0.8.5 tag measurement.
- `repo.md` — current status (v0.8.5), all documented commands stated as
  implemented, audit appendix relabelled as v0.5 history with ADR-0030 as the
  current policy.
- `docs/evidence/v085-http-cache.md`, `docs/evidence/v085-hot-secrets-drift.md`
  — Rust Audit cells corrected to `exit 1` with the reason, keeping the
  three reviewed R16 false positives.
- `docs/evidence/v085-rust-audit-triage.md` — declares the current gate result
  (7 high → exit 1) and why no rule is suppressed.
- `eqproxy-config/src/import/report.rs` + `import/mod.rs` — the header bucket
  is now labelled «con aviso» and the `Counts.review` contract says it groups
  `Review` + `Info`; the detail sections keep them separate. Numbers and the
  fuzz invariants (header == counts, one bullet per classification) are
  unchanged. The CLI's own summary line (`main.rs:1115`) said "to review" for
  the same combined number and now says "with notes"; no test asserted the old
  wording.

## 3. Commands run

| Command | Result |
|---|---|
| `grep -c -i hyper Cargo.lock` | `0`; `grep -rn -i hyper eqproxy-*/src` empty |
| `cargo test --workspace -- --list` | exit 0; 659 cases, 31 binaries |
| `cargo test --workspace` | **exit 0; 657 passed, 0 failed, 2 ignored** |
| `git worktree add --detach target/wt-v085 v0.8.5` + `cargo test --workspace -- --list` there | exit 0; **654 cases, 31 binaries** (tag `6e938ae`) |
| `python3.13 <rust-ai-audit>/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | **exit 1**; 0 critical / 7 high / 397 medium / 107 low — `report/rust-audit-20261031-225405.md` (local-only) |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `.github/scripts/check-evidence-test-citations.py` | exit 0; every test name cited in `docs/evidence/` exists |

## 4. Not done in this increment

- **No refactor of the monoliths** (`validate::check`, `serve_http`, `main.rs`,
  `admin.rs`): real work, but it changes code and needs its own increment.
- **No `[profile.release]` tuning**: it would invalidate every published
  benchmark; it is a decision, not a typo.
- **The admin plane keeps its own `read_head`/`content_length_of`**: whether to
  extract a shared minimal parser or document the duplication as intentional is
  still open.
- **The importer output and report remain in Spanish**, while the published
  policy says documentation in English. Weakening one or the other is a product
  decision (the fuzz target parses the Spanish header), so it is recorded, not
  silently changed.
- **Fuzz targets are still not in CI**; their evidence stays manual.
- **The panel cookie still has no `Secure`** attribute: ADR-0029 documents
  `HttpOnly; SameSite=Strict; Path=/panel`, and a TLS-fronted admin listener
  would not get the flag. Left as documented, not silently changed.
- **Audits**: Veyra Proof, CodeGraph and Docker are reported in §5 exactly as
  they ran.

## 5. Audits of this increment

| Leg | Command | Result | Evidence |
|---|---|---|---|
| Veyra Proof | `veyra audit . --profile standard` | **ACCEPTED, exit 0**; no problems. `dylint` and `cargo-semver-checks` skipped (no config / no baseline), `cargo-geiger` INCONCLUSIVE (coverage warnings), all three for the reasons in ADR-0030. `cargo test` inside the run: 657 passed, 0 failed, 2 ignored | `.veyra/evidence/20261031T215715Z/report.md` |
| Rust Audit | `python3.13 <rust-ai-audit>/scripts/rust_audit.py scan . --format md --out report --fail-on high` (0.2.7) | **exit 1**; 0 critical / 7 high, all seven triaged as false positives (3 R16 in `cache.rs`, 4 R09 in the fuzz target) | `report/rust-audit-20261031-225405.md` (local-only) + [v085-rust-audit-triage.md](v085-rust-audit-triage.md) |
| CodeGraph Rust | `codegraph-rust index . --stats-json /tmp/cg-docs-truth.json --batch-size 16` + `agent context "where does the configuration importer print the counts header"` | **exit 0**; 80 files, 77 147 lines, 8 438 nodes, 14 876 edges, `graph_complete: true`, `vector_index_ready: true`, `errors: 0`, `index_ms: 116 596`, resolution 53.6 %. The query pointed at `import/report.rs:295-303` and at a second surface, the CLI line at `main.rs:1115`; both were checked in the source, and the CLI one was fixed. A second index pass over the tree after that one-string edit **hit the tool's 600 s limit** and wrote no stats file (the store still answers); the cited numbers are from the first pass | `/tmp/cg-docs-truth.json` |
| Hindsight | `hindsight-note --title ... --project EQProxy` | **NOT EXECUTED** — no note was written for this pass | — |

The CodeGraph `agent` answer also named `eqproxy-cli/tests/cli_e2e.rs:484` as an
assertion on the report prefix; that file was checked and indeed asserts
`"Directivas leídas:"`, which this change keeps. Nothing else the answer said
was used without a source check.

**Veyra delta.** The run above finished before the `main.rs:1115` wording
change. That delta is one string literal in a `println!`; after it,
`cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and
`cli_e2e::config_import_converts_and_names_what_it_refuses` were re-run (all
exit 0). The Veyra verdict is therefore reported for the tree one literal
before the final one, with the delta named, not as a re-audit of the final
tree.
