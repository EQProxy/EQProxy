# Rust Audit — triage of the seven `high` findings

**Scan.** Rust Audit 0.2.7
(`/Users/toni/.agents/skills/rust-audit/scripts/rust_audit.py`, sha256
`84b5ba5ba05931e7e318b1ee345ffe32f18cb473c051b33066f5938f0fb79b26`),
report `report/rust-audit-20261031-175609.md` (local-only, like every audit
report: `/report/` is in `.git/info/exclude`), run 2026-10-31 17:56 over the
working tree at `04859af` (only the CodeGraph evidence page ahead of the
release commit; no Rust differs from `fccc747`). The scan was repeated over the
final tree after the `Server: eqproxy` change
(`report/rust-audit-20261031-222940.md`, 22:29) with **the same counts
(0 critical, 7 high, 397 medium, 107 low) and the same seven locations**, so
this triage covers the current tree.

| Severity | Count |
|---|---|
| critical | 0 |
| high | 7 |
| medium | 397 |
| low | 107 |

`--fail-on high` therefore exits **1** on this tree (re-verified 2026-10-31
22:54: `report/rust-audit-20261031-225405.md`, the same seven locations). The
gate is red by rule, not because of an unreviewed defect: the seven are
triaged below and no rule is suppressed. Making the gate green would require
either excluding the fuzz workspace from the scan or an inline suppression
policy — an owner decision, not something to hide.

`AGENTS.md` requires every critical/high finding to be named as real or as a
false positive with the evidence that shows it. This page does that for the
seven `high` ones. The 397 `medium` and 107 `low` findings are not individually
triaged here; they are the tool's style-and-idiom battery and were not part of
this gate.

| # | Rule | Location | Verdict |
|---|---|---|---|
| 1 | R16 | `eqproxy-net/src/cache.rs:421` `refresh` | false positive |
| 2 | R16 | `eqproxy-net/src/cache.rs:559` `insert` | false positive |
| 3 | R16 | `eqproxy-net/src/cache.rs:624` `remove` | false positive |
| 4 | R09 | `fuzz/fuzz_targets/config_import.rs:43` | false positive (fuzz harness) |
| 5 | R09 | `fuzz/fuzz_targets/config_import.rs:55` | false positive (fuzz harness) |
| 6 | R09 | `fuzz/fuzz_targets/config_import.rs:85` | false positive (fuzz harness) |
| 7 | R09 | `fuzz/fuzz_targets/config_import.rs:95` | false positive (fuzz harness) |

## R16 — "holds a synchronous Mutex guard across `.await`"

The rule's evidence is the textual pair `lock() … .await` inside the function.
In all three functions the guard lives in a **block that closes before the
await**, and the tool does not model the block:

- `refresh` — `let mut inner = self.lock();` at `cache.rs:475`, the block closes
  at `:489`, and the only await in that region is
  `tokio::task::spawn_blocking(…).await` at `:496`. The later
  `let stored = self.lock()…` at `:502-507` is a temporary guard dropped at the
  end of that statement, so `persist_variant(…).await` at `:509` runs unlocked.
- `insert` — guard at `:585`, block closes at `:610`,
  `spawn_blocking(…).await` at `:618`.
- `remove` — guard at `:626` inside `let paths: Vec<PathBuf> = { … };`, which
  closes at `:641`; the `.await` is at `:648`.

The design is deliberate: `Cache` guards a `std::sync::Mutex<Store>` whose
critical sections are pure in-memory bookkeeping (map, byte counter, LRU
order); every disk or blocking operation is moved out of the guard and into
`spawn_blocking`, which is awaited only after the guard is gone. That is also
why the type is `std::sync::Mutex` and not `tokio::sync::Mutex`: no guard ever
needs to survive an await point.

The two other lock users confirm the pattern instead of contradicting it:
`Cache::lookup` (`:333`) is a **synchronous** function with no await at all,
and `Cache::wait` (`:378`) takes `shared.result.lock()` only **after** its
`tokio::time::timeout(…).await` returned, then copies the value out.

No `await_holding_lock` suppression exists anywhere in the tree
(`grep -rn await_holding_lock --include=*.rs .` is empty), so the clean Clippy
run is not masking the pattern either.

## R09 — `panic!`/`expect` "in production code"

All four live in `fuzz/fuzz_targets/config_import.rs`, the libFuzzer target of
the configuration importer, which `cargo test --workspace` does not build or
run (it is a separate workspace, `fuzz/Cargo.toml`). In a fuzz target a panic
**is** the failure signal: libFuzzer turns it into a crash artifact with the
input that caused it. The four sites are the target's own oracles:

- `:43` — the emitted TOML must parse; a conversion that emits unparsable TOML
  is the bug the target exists to find.
- `:55` — conversion must be deterministic; a second run on the same input must
  produce the same TOML (`expect("deterministic conversion")`).
- `:85` — the report must have its counts header.
- `:95` — that header must carry the five expected numbers.

Replacing these with `?` would make the harness pass on exactly the inputs it
must reject. They are not reachable from the `eqproxy` binary: the crate is not
a dependency of `eqproxy-cli`, and no production path includes this file.

## What this record does and does not change

- It changes no code: all seven findings are classified, not fixed, because
  they do not describe a defect.
- It is not a blanket clearance of `cache.rs`. If a future change moves I/O
  inside one of those blocks, the finding becomes real; the guard scoping above
  is the invariant to preserve.
- The scan's `medium`/`low` volume is unchanged and stays as the tool's
  advisory backlog.
