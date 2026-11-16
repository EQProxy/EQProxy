#!/usr/bin/env bash
# Rust Audit gate for EQProxy.
#
# The reviewed baseline is versioned at rust-audit-baseline.json. This wrapper
# owns the gate arguments and never lets a caller replace or omit the baseline.
# It also checks the raw report by fingerprint *and occurrence count*: Rust
# Audit's native set-based diff would not see a second high finding with the
# same fingerprint, while this gate does.
#
# The tool can come from either:
#   RUST_AUDIT_BIN=/path/to/rust-audit       (CI: pinned wheel in a venv)
#   RUST_AUDIT_HOME=/path/to/rust-ai-audit   (local checkout)
# Otherwise an installed `rust-audit` on PATH is used.
set -euo pipefail

ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
BASELINE="$ROOT/rust-audit-baseline.json"
CHECKER="$ROOT/.github/scripts/check-rust-audit-highs.py"
PYTHON=${PYTHON:-python3}
LOCAL_PYTHON=${RUST_AUDIT_PYTHON:-python3.13}
REPORT_DIR=${RUST_AUDIT_OUT:-"$ROOT/report"}

if (($#)); then
    cat >&2 <<EOF
error: the Rust Audit gate takes no scan arguments; it owns --format,
       --out, --fail-on high and --baseline.
       Remove: $*
EOF
    exit 2
fi

if [[ ! -f "$BASELINE" ]]; then
    cat >&2 <<EOF
error: reviewed Rust Audit baseline not found: $BASELINE

The baseline is versioned with the repository. A missing file means the gate
cannot run; do not create an empty baseline and do not skip the leg.
See docs/rust-audit-runbook.md.
EOF
    exit 2
fi

if [[ ! -f "$CHECKER" ]]; then
    echo "error: Rust Audit high checker not found: $CHECKER" >&2
    exit 2
fi

AUDIT=()
if [[ -n "${RUST_AUDIT_BIN:-}" ]]; then
    if [[ ! -x "$RUST_AUDIT_BIN" ]]; then
        echo "error: RUST_AUDIT_BIN is not executable: $RUST_AUDIT_BIN" >&2
        exit 2
    fi
    AUDIT=("$RUST_AUDIT_BIN")
elif [[ -n "${RUST_AUDIT_HOME:-}" ]]; then
    if [[ ! -f "$RUST_AUDIT_HOME/scripts/rust_audit.py" ]]; then
        echo "error: RUST_AUDIT_HOME has no scripts/rust_audit.py: $RUST_AUDIT_HOME" >&2
        exit 2
    fi
    AUDIT=("$LOCAL_PYTHON" "$RUST_AUDIT_HOME/scripts/rust_audit.py")
elif [[ -f /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py ]]; then
    AUDIT=("$LOCAL_PYTHON" /Volumes/Lexar/Cursor/rust-ai-audit/scripts/rust_audit.py)
elif command -v rust-audit >/dev/null 2>&1; then
    AUDIT=(rust-audit)
else
    cat >&2 <<EOF
error: Rust Audit not found.

Set RUST_AUDIT_BIN to the installed CLI, or RUST_AUDIT_HOME to a rust-ai-audit
checkout. CI installs the pinned wheel from
.github/scripts/rust-audit-requirements.txt.
EOF
    exit 2
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/eqproxy-rust-audit.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# Raw scan: no --baseline, so the strict checker sees every high occurrence.
# Exit 1 is expected while the reviewed high false positives still exist; 0 is
# possible after they are fixed; anything above 1 is an execution failure.
RAW_DIR="$TMP/raw"
mkdir -p "$RAW_DIR"
set +e
"${AUDIT[@]}" scan "$ROOT" \
    --format json --out "$RAW_DIR" --no-gap-report >"$TMP/raw.log" 2>&1
raw_status=$?
set -e

if ((raw_status > 1)); then
    cat "$TMP/raw.log" >&2
    echo "error: raw Rust Audit scan failed with exit $raw_status" >&2
    exit 2
fi

shopt -s nullglob
raw_reports=("$RAW_DIR"/rust-audit-*.json)
shopt -u nullglob
if ((${#raw_reports[@]} != 1)); then
    cat "$TMP/raw.log" >&2
    echo "error: raw scan wrote ${#raw_reports[@]} JSON reports, expected 1" >&2
    exit 2
fi

set +e
"$PYTHON" "$CHECKER" --report "${raw_reports[0]}" --baseline "$BASELINE"
check_status=$?
set -e

# Produce the normal Markdown evidence report with the native baseline. This
# also catches a new fingerprint in the tool's own comparison; the strict
# checker above additionally catches duplicate occurrences. Run it even when
# the checker fails so the failure has a report attached.
set +e
"${AUDIT[@]}" scan "$ROOT" \
    --format md --out "$REPORT_DIR" --fail-on high --baseline "$BASELINE"
report_status=$?
set -e

if ((check_status != 0)); then
    exit "$check_status"
fi
case "$report_status" in
    0) exit 0 ;;
    1) exit 1 ;;
    *)
        echo "error: Rust Audit report scan failed with exit $report_status" >&2
        exit 2
        ;;
esac
