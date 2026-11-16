#!/usr/bin/env python3
"""Strict high-finding check for the Rust Audit CI gate.

Rust Audit's native ``--baseline`` diff compares *sets* of fingerprints. That
is the right identity for line moves, but it cannot see a second occurrence
with the same fingerprint: a new ``Command::new(`` in the same function would
look like the already-reviewed one. This checker keeps the same fingerprint
identity and compares **counts**, so an added occurrence fails even when the
fingerprint already exists.

The baseline is versioned in the repository and contains only reviewed
``high`` false positives. Each entry must carry its review and an
``expected_count`` (default 1). The checker fails on:

* a current high/critical finding whose fingerprint is not in the baseline;
* more occurrences of a reviewed fingerprint than its expected count;
* a reviewed fingerprint that disappeared (the baseline is stale and must be
  updated, so it cannot silently become a future allow-list);
* malformed/missing input, a tool/version mismatch or an empty baseline.

Exit codes: 0 = exact match, 1 = new/extra/stale finding, 2 = malformed input
or a gate that could not run.

Usage:
    python3 .github/scripts/check-rust-audit-highs.py \\
        --report /tmp/rust-audit.json --baseline rust-audit-baseline.json
    python3 .github/scripts/check-rust-audit-highs.py --self-test
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from collections import Counter
from pathlib import Path

HIGH_SEVERITIES = {"critical", "high"}
FINGERPRINT = re.compile(r"^[0-9a-f]{16}$")


class GateError(Exception):
    """The report, baseline or its metadata cannot be checked."""


def _load_json(path: Path, label: str) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise GateError(f"cannot read {label} {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise GateError(f"{label} {path} is not a JSON object")
    return payload


def _check_metadata(report: dict, baseline: dict) -> None:
    report_version = report.get("version")
    baseline_version = baseline.get("version")
    if report_version and baseline_version and report_version != baseline_version:
        raise GateError(
            f"Rust Audit version mismatch: report={report_version}, "
            f"baseline={baseline_version}; review the baseline for the new engine"
        )

    report_engine = report.get("engine")
    baseline_engine = baseline.get("engine")
    if isinstance(report_engine, dict) and isinstance(baseline_engine, dict):
        report_sha = report_engine.get("sha256")
        baseline_sha = baseline_engine.get("sha256")
        if report_sha and baseline_sha and report_sha != baseline_sha:
            raise GateError(
                "Rust Audit engine mismatch: report sha256="
                f"{report_sha}, baseline sha256={baseline_sha}; "
                "review the baseline for the new engine"
            )


def _reviewed_entries(baseline: dict) -> tuple[Counter, dict[str, dict]]:
    findings = baseline.get("findings")
    if not isinstance(findings, list) or not findings:
        raise GateError(
            "baseline has no findings; an empty baseline would hide every high"
        )

    expected: Counter = Counter()
    entries: dict[str, dict] = {}
    for index, item in enumerate(findings, 1):
        if not isinstance(item, dict):
            raise GateError(f"baseline finding #{index} is not an object")
        rule = item.get("rule_id")
        severity = item.get("severity")
        file = item.get("file")
        fingerprint = item.get("fingerprint")
        review = item.get("review")
        if not isinstance(rule, str) or not rule.strip():
            raise GateError(f"baseline finding #{index} has no rule_id")
        if severity != "high":
            raise GateError(
                f"baseline finding #{index} ({rule}) must be high; "
                "critical findings are never allowlisted"
            )
        if not isinstance(file, str) or not file.strip():
            raise GateError(f"baseline finding #{index} ({rule}) has no file")
        if not isinstance(fingerprint, str) or not FINGERPRINT.fullmatch(fingerprint):
            raise GateError(
                f"baseline finding #{index} ({rule}) has an invalid fingerprint"
            )
        if not isinstance(review, str) or not review.strip():
            raise GateError(
                f"baseline finding #{index} ({rule}) has no review; "
                "only reviewed false positives may be allowlisted"
            )
        count = item.get("expected_count", 1)
        if isinstance(count, bool) or not isinstance(count, int) or count < 1:
            raise GateError(
                f"baseline finding #{index} ({rule}) has invalid expected_count"
            )
        if fingerprint in expected:
            raise GateError(
                f"baseline has duplicate fingerprint {fingerprint}; "
                "combine its expected_count"
            )
        expected[fingerprint] = count
        entries[fingerprint] = item
    return expected, entries


def _current_highs(report: dict) -> tuple[Counter, dict[str, dict]]:
    findings = report.get("findings")
    if not isinstance(findings, list):
        raise GateError("report has no findings list")

    current: Counter = Counter()
    examples: dict[str, dict] = {}
    for item in findings:
        if not isinstance(item, dict):
            raise GateError("report contains a non-object finding")
        severity = str(item.get("severity", "")).lower()
        if severity not in HIGH_SEVERITIES:
            continue
        fingerprint = item.get("fingerprint")
        if not isinstance(fingerprint, str) or not FINGERPRINT.fullmatch(fingerprint):
            raise GateError(
                "a high/critical finding has no valid fingerprint: "
                f"{item.get('rule_id')} {item.get('file')}:{item.get('line')}"
            )
        current[fingerprint] += 1
        examples.setdefault(fingerprint, item)
    return current, examples


def _describe(item: dict) -> str:
    return (
        f"{item.get('rule_id', '?')} {item.get('file', '?')}:"
        f"{item.get('line', '?')} in `{item.get('function', '')}`"
        f" — {item.get('evidence', '')}"
    )


def check_data(report: dict, baseline: dict) -> list[str]:
    """Return human-readable gate problems; an empty list means PASS."""
    _check_metadata(report, baseline)
    expected, entries = _reviewed_entries(baseline)
    current, examples = _current_highs(report)

    problems: list[str] = []
    for fingerprint, count in sorted(current.items()):
        if fingerprint not in expected:
            item = examples[fingerprint]
            problems.append(
                f"NEW {item.get('severity')} {_describe(item)} "
                f"(fingerprint {fingerprint}, {count} occurrence(s))"
            )

    for fingerprint, expected_count in sorted(expected.items()):
        actual_count = current.get(fingerprint, 0)
        if actual_count > expected_count:
            item = entries[fingerprint]
            problems.append(
                f"EXTRA occurrence of reviewed {_describe(item)} "
                f"(fingerprint {fingerprint}): expected {expected_count}, "
                f"found {actual_count}"
            )
        elif actual_count < expected_count:
            item = entries[fingerprint]
            problems.append(
                f"STALE baseline entry {_describe(item)} "
                f"(fingerprint {fingerprint}): expected {expected_count}, "
                f"found {actual_count}; remove or re-review it"
            )
    return problems


def _self_test() -> int:
    fingerprint = "a" * 16
    baseline = {
        "tool": "Rust Audit",
        "version": "0.2.9",
        "engine": {"sha256": "b" * 64},
        "findings": [
            {
                "rule_id": "R21",
                "severity": "high",
                "file": "build.rs",
                "line": 14,
                "function": "main",
                "evidence": "Command::new(",
                "fingerprint": fingerprint,
                "expected_count": 1,
                "review": "reviewed false positive",
            }
        ],
    }
    report = {
        "tool": "Rust Audit",
        "version": "0.2.9",
        "engine": {"sha256": "b" * 64},
        "findings": [
            {
                "rule_id": "R21",
                "severity": "high",
                "file": "build.rs",
                "line": 14,
                "function": "main",
                "evidence": "Command::new(",
                "fingerprint": fingerprint,
            }
        ],
    }

    if check_data(report, baseline):
        print("selftest FAIL: exact reviewed finding was rejected", file=sys.stderr)
        return 1

    duplicate = copy.deepcopy(report)
    duplicate["findings"].append(copy.deepcopy(report["findings"][0]))
    problems = check_data(duplicate, baseline)
    if not any(problem.startswith("EXTRA occurrence") for problem in problems):
        print("selftest FAIL: duplicate occurrence was not detected", file=sys.stderr)
        return 1

    new = copy.deepcopy(report)
    new["findings"][0]["fingerprint"] = "c" * 16
    if not any(problem.startswith("NEW ") for problem in check_data(new, baseline)):
        print("selftest FAIL: new fingerprint was not detected", file=sys.stderr)
        return 1

    missing = copy.deepcopy(report)
    missing["findings"] = []
    if not any(
        problem.startswith("STALE baseline entry")
        for problem in check_data(missing, baseline)
    ):
        print("selftest FAIL: stale baseline entry was not detected", file=sys.stderr)
        return 1

    medium = copy.deepcopy(report)
    medium["findings"][0]["severity"] = "medium"
    if not any(
        problem.startswith("STALE baseline entry")
        for problem in check_data(medium, baseline)
    ):
        print("selftest FAIL: medium finding was counted as high", file=sys.stderr)
        return 1

    unreviewed = copy.deepcopy(baseline)
    unreviewed["findings"][0]["review"] = ""
    try:
        check_data(report, unreviewed)
    except GateError:
        pass
    else:
        print("selftest FAIL: unreviewed baseline entry was accepted", file=sys.stderr)
        return 1

    wrong_version = copy.deepcopy(report)
    wrong_version["version"] = "0.2.8"
    try:
        check_data(wrong_version, baseline)
    except GateError:
        pass
    else:
        print("selftest FAIL: engine version mismatch was accepted", file=sys.stderr)
        return 1

    print("selftest PASS: exact, new, duplicate, stale and malformed cases")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Strict reviewed-high check for the Rust Audit gate"
    )
    parser.add_argument("--report", type=Path, help="raw Rust Audit JSON report")
    parser.add_argument("--baseline", type=Path, help="reviewed baseline JSON")
    parser.add_argument("--self-test", action="store_true", help="run synthetic checks")
    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()
    if args.report is None or args.baseline is None:
        parser.error("--report and --baseline are required without --self-test")

    try:
        report = _load_json(args.report, "report")
        baseline = _load_json(args.baseline, "baseline")
        problems = check_data(report, baseline)
    except GateError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if problems:
        print("Rust Audit high gate FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    current, _ = _current_highs(report)
    reviewed = sum(
        int(item.get("expected_count", 1))
        for item in baseline["findings"]
        if item.get("severity") == "high"
    )
    print(
        "Rust Audit high gate PASS: "
        f"{sum(current.values())} high finding(s) exactly match "
        f"{reviewed} reviewed high occurrence(s); 0 new, 0 extra, 0 stale."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
