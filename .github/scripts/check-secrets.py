#!/usr/bin/env python3
"""Refuse private keys and provider credentials in published content.

The gate has three modes:

* default: scan the tracked tree, which is what CI publishes;
* ``--staged``: scan blobs in the index, so a pre-commit hook catches a key
  before it becomes a commit;
* ``--range REV``: scan every commit added in ``REV`` (for example
  ``origin/main..HEAD``) for secrets on added lines and in commit messages.

The range mode is deliberately scoped to additions. A key that was added and
deleted in the same change is still caught, because the commit that added it is
in the range; old test keys already present on the base branch are not
re-reported as if the current change had introduced them.

There is no allowlist: a private key is never acceptable in published files.
Public certificates (`BEGIN CERTIFICATE`) are not matches. The gate looks for
shapes that are credentials by construction, not for generic ``token = ...``
assignments, which are often test values.

Usage:
    python3 .github/scripts/check-secrets.py
    python3 .github/scripts/check-secrets.py --staged
    python3 .github/scripts/check-secrets.py --range origin/main..HEAD
    python3 .github/scripts/check-secrets.py --self-test
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAX_BYTES = 2_000_000

RULES = [
    (
        "private-key",
        re.compile(
            rb"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED |SSH2 ENCRYPTED )?PRIVATE KEY-----"
            rb"|-----BEGIN PGP " + b"PRIVATE KEY" + b" BLOCK-----"
        ),
    ),
    ("aws-access-key", re.compile(rb"AKIA[0-9A-Z]{16}")),
    ("github-token", re.compile(rb"gh[pousr]_[A-Za-z0-9]{30,}")),
    ("github-pat", re.compile(rb"github_pat_[A-Za-z0-9_]{20,}")),
    ("openai-key", re.compile(rb"sk-[A-Za-z0-9]{20,}")),
    ("slack-token", re.compile(rb"xox[baprs]-[A-Za-z0-9-]{10,}")),
    ("google-api-key", re.compile(rb"AIza[0-9A-Za-z_-]{35}")),
    ("jwt", re.compile(rb"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.")),
]


def scan_bytes(data: bytes) -> list[str]:
    return [name for name, pattern in RULES if pattern.search(data)]


def git(*args: str) -> bytes:
    try:
        return subprocess.run(
            ["git", *args],
            cwd=ROOT,
            check=True,
            capture_output=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as err:
        detail = ""
        if isinstance(err, subprocess.CalledProcessError) and err.stderr:
            detail = ": " + err.stderr.decode("utf-8", "replace").strip()
        raise SystemExit(f"cannot run git {' '.join(args)}{detail}") from err


def split_z(out: bytes) -> list[str]:
    return [item.decode("utf-8", "surrogateescape") for item in out.split(b"\0") if item]


def tracked_files() -> list[str]:
    return split_z(git("ls-files", "-z"))


def staged_files() -> list[str]:
    return split_z(git("diff", "--cached", "--name-only", "--diff-filter=AM", "-z"))


def added_lines(commit: str) -> bytes:
    patch = git("show", "--format=", "--no-ext-diff", "--unified=0", commit)
    lines = []
    for line in patch.splitlines():
        if line.startswith(b"+") and not line.startswith(b"+++"):
            lines.append(line[1:])
    return b"\n".join(lines)


def scan_blob(label: str, data: bytes) -> int:
    if len(data) > MAX_BYTES or b"\0" in data[:4096]:
        return 0
    findings = 0
    for rule in scan_bytes(data):
        print(f"::error file={label}::possible secret ({rule})")
        findings += 1
    return findings


def scan_tracked() -> int:
    findings = 0
    for path in tracked_files():
        full = ROOT / path
        try:
            if not full.is_file():
                continue
            data = full.read_bytes()
        except OSError as err:
            print(f"cannot read {path}: {err}", file=sys.stderr)
            findings += 1
            continue
        findings += scan_blob(path, data)
    return findings


def scan_staged() -> int:
    findings = 0
    for path in staged_files():
        findings += scan_blob(path, git("show", f":{path}"))
    return findings


def scan_range(rev_range: str) -> int:
    findings = 0
    commits = git("rev-list", "--reverse", rev_range).decode().split()
    for commit in commits:
        message = git("log", "-1", "--format=%B", commit)
        for rule in scan_bytes(message):
            print(f"::error::possible secret ({rule}) in commit message {commit[:12]}")
            findings += 1
        findings += scan_blob(f"{commit[:12]} (added lines)", added_lines(commit))
    return findings


def self_test() -> None:
    assert scan_bytes(b"AKIA" + b"IOSFODNN7EXAMPLE") == ["aws-access-key"]
    # Concatenated so this source file does not contain the literal header it
    # is designed to reject; the gate scans itself once it is committed.
    assert "private-key" in scan_bytes(b"-----BEGIN " + b"PRIVATE KEY-----\n")
    assert "private-key" in scan_bytes(
        b"-----BEGIN " + b"SSH2 ENCRYPTED PRIVATE KEY-----\n"
    )
    assert "private-key" in scan_bytes(
        b"-----BEGIN PGP " + b"PRIVATE KEY" + b" BLOCK-----\n"
    )
    assert scan_bytes(b"-----BEGIN CERTIFICATE-----\n") == []
    assert scan_bytes(b"the quick brown fox") == []
    print("self-test OK")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--staged", action="store_true")
    mode.add_argument("--range", dest="rev_range", metavar="REV")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    if args.staged:
        findings = scan_staged()
        if findings:
            print(f"{findings} possible secret(s) in staged files")
            return 1
        print("no secrets in staged files")
        return 0

    if args.rev_range:
        findings = scan_range(args.rev_range)
        if findings:
            print(f"{findings} possible secret(s) added in {args.rev_range}")
            return 1
        print(f"no secrets added in {args.rev_range}")
        return 0

    findings = scan_tracked()
    if findings:
        print(f"{findings} possible secret(s) in tracked files")
        return 1
    print("no secrets in tracked files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
