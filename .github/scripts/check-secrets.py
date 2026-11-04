#!/usr/bin/env python3
"""Refuse private keys and provider credentials in tracked files.

The gate scans `git ls-files`, so it covers the published tree and ignores
build output. It looks for shapes that are credentials by construction:
PEM private keys, AWS access key ids, GitHub/OpenAI/Slack/Google tokens and
JWTs. There is no allowlist: a private key is never acceptable in the tracked
tree. Public certificates (`BEGIN CERTIFICATE`) are not matches.

This gate covers the current tree, not history or tags. The release workflow
checks the exported source tarball separately.

A generic `token = "..."` assignment is not a rule: test tokens are not
credentials to an external service, and a broad rule would only train us to
allowlist them. The gate is about keys that unlock something.

Usage:
    python3 .github/scripts/check-secrets.py
    python3 .github/scripts/check-secrets.py --self-test
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

RULES = [
    ("private-key", re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----")),
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


def tracked_files() -> list[str]:
    try:
        out = subprocess.run(
            ["git", "ls-files", "-z"],
            cwd=ROOT,
            check=True,
            capture_output=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as err:
        raise SystemExit(f"cannot list tracked files: {err}") from err
    return [item.decode("utf-8", "surrogateescape") for item in out.split(b"\0") if item]


def self_test() -> None:
    assert scan_bytes(b"AKIA" + b"IOSFODNN7EXAMPLE") == ["aws-access-key"]
    # Concatenated so this source file does not contain the literal header it
    # is designed to reject; the gate scans itself once it is committed.
    assert "private-key" in scan_bytes(b"-----BEGIN " + b"PRIVATE KEY-----\n")
    assert scan_bytes(b"-----BEGIN CERTIFICATE-----\n") == []
    assert scan_bytes(b"the quick brown fox") == []
    print("self-test OK")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0

    findings = 0
    for path in tracked_files():
        full = ROOT / path
        try:
            if not full.is_file() or full.stat().st_size > 2_000_000:
                continue
            data = full.read_bytes()
        except OSError as err:
            print(f"cannot read {path}: {err}", file=sys.stderr)
            findings += 1
            continue
        if b"\0" in data[:4096]:
            continue
        for rule in scan_bytes(data):
            print(f"::error file={path}::possible secret ({rule})")
            findings += 1

    if findings:
        print(f"{findings} possible secret(s) in tracked files")
        return 1
    print("no secrets in tracked files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
