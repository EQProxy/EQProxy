# Rust Audit gate runbook

## Invariant

The Rust Audit gate is `.github/scripts/rust-audit-gate.sh`. It takes no scan
arguments and always applies the versioned reviewed baseline at
`rust-audit-baseline.json` with `--fail-on high`. Do not run
`rust_audit.py scan` directly for the gate, and do not pass `--baseline` by
hand: the wrapper owns the arguments so the reviewed baseline cannot be
replaced, omitted or weakened.

The baseline contains only reviewed high findings whose source evidence shows a
false positive. It is line-independent and keyed by fingerprint. The wrapper
also runs
[`.github/scripts/check-rust-audit-highs.py`](../.github/scripts/check-rust-audit-highs.py)
over a raw JSON scan, which compares **occurrence counts**, not just set
membership: a second finding with the same fingerprint fails even though Rust
Audit's native set diff would treat it as unchanged.

The gate is blocking on `critical`/`high` only. It does not turn the existing
`medium`/`low` backlog into a gate; those findings are still printed in the raw
scan and in the report, but a change does not fail merely for adding one. A
future decision to gate them needs its own reviewed baseline, not this file.

## Files

| Path | Published? | Purpose |
|---|---|---|
| `.github/scripts/rust-audit-gate.sh` | yes, tracked | Gate wrapper; owns the baseline, `--fail-on high` and the two scans. |
| `.github/scripts/check-rust-audit-highs.py` | yes, tracked | Strict fingerprint + occurrence-count checker and self-test. |
| `.github/scripts/rust-audit-requirements.txt` | yes, tracked | Pinned Rust Audit wheel version and SHA-256 for CI. |
| `rust-audit-baseline.json` | yes, tracked | The six reviewed high false positives, each with `review` and `expected_count`. |
| `rust-audit.toml` | **no**, local-only (`.git/info/exclude`) | Optional local exclusion set for old worktrees/artifacts. |

A clean clone has the baseline and therefore needs no maintainer-local file to
run the gate. `rust-audit.toml` is not required in CI: the default exclusions
cover `target/`, `.git/` and `report/`, and the clean clone contains no old
worktree.

## Run

CI uses the `rust-audit` job in `.github/workflows/ci.yml`. It installs the
wheel from the pinned requirements file with `--require-hashes`, runs
`rust-audit selftest`, self-tests the checker and then runs the gate with
`RUST_AUDIT_BIN` pointing at the venv CLI.

Local run with the rust-ai-audit checkout:

```bash
PYTHON=python3.13 \
RUST_AUDIT_HOME=/path/to/rust-ai-audit \
  .github/scripts/rust-audit-gate.sh
```

Local run with the installed CLI:

```bash
RUST_AUDIT_BIN="$(command -v rust-audit)" \
  .github/scripts/rust-audit-gate.sh
```

`PYTHON` defaults to `python3` for the stdlib-only checker. The tool itself
needs Python 3.11+ when `RUST_AUDIT_HOME` is used; set `RUST_AUDIT_PYTHON`
(default `python3.13`) when the checkout needs another interpreter. On this
machine the checkout is `/Volumes/Lexar/Cursor/rust-ai-audit`, which is the
last fallback when neither `RUST_AUDIT_BIN` nor `RUST_AUDIT_HOME` is set.

The wrapper writes the Markdown report and the not-executed report under
`report/` (local-only). It runs a raw JSON scan to a temporary directory first,
then the normal baseline report.

| Exit | Meaning |
|---|---|
| `0` | Exactly the reviewed high findings are present; no new, extra or stale entry. |
| `1` | A new high, an extra occurrence of a reviewed high, or a stale baseline entry. The gate is red. |
| `2` | The gate could not run: missing/broken baseline or report, engine mismatch, missing tool, or a scan execution error. **NOT EXECUTED**, never a pass. |

The raw scan may exit `1` while the six reviewed high false positives still
exist; the checker turns that into `0` only when the counts match exactly.
Anything above `1` from the raw scan is an execution failure.

## Updating the baseline

1. Run a raw JSON scan and read every `critical`/`high` finding on the real
   tree.
2. Review each one against the source. Only a reviewed false positive may be
   added. Record the `review` and an `expected_count` in the baseline.
3. Keep `tool`, `version` and `engine.sha256` in step with the pinned wheel in
   `.github/scripts/rust-audit-requirements.txt`; the checker rejects a
   version/hash mismatch.
4. Run the gate. It must print `0 new, 0 extra, 0 stale`. A new high fails; a
   finding that no longer appears also fails as `STALE baseline entry`, so the
   entry must be removed or re-reviewed instead of silently becoming a future
   allow-list.
5. Do not add a `critical`, an empty `review`, a duplicate fingerprint or an
   empty baseline. The checker rejects all four.

Never update the baseline merely to make the gate green. A baseline entry is a
reviewed exception, not a suppression mechanism.

## Residual limit

The fingerprint is `rule + file + function + normalized evidence`; it is
deliberately line-independent. The count check catches an **added** occurrence
of a reviewed fingerprint. If one occurrence disappears and a new occurrence
with exactly the same fingerprint appears at another line in the same function,
the fingerprint alone cannot distinguish the replacement. That is why every
entry keeps its source review and why a stale entry fails instead of being
left as a permanent allow-list; a human must still review the diff.

## Related process

- `AGENTS.md`, Leg 2, defines the audit leg and delivery requirements.
- `CONTRIBUTING.md`, “Audits”, is the published contributor-facing summary.
- `.github/scripts/check-rust-audit-highs.py` documents the fingerprint/count
  contract and carries its own synthetic self-test.
