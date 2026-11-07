#!/usr/bin/env python3
"""Refuse a release image build that does not select the ``runtime`` stage.

``docker/Dockerfile`` ends with ``FROM runtime AS profile``, so ``docker
build`` without ``--target`` does not build the product image: it builds the
**last** stage, the profiling one. That stage carries ``EQPROXY_PPROF_DIR``,
``EQPROXY_PROFILE_SAMPLES`` and mimalloc's ``MIMALLOC_SHOW_STATS=1`` plus
dump-on-exit, so the tagged bytes run a profiled binary. v0.9.3 shipped that
way until the release workflow was fixed; a green build proves nothing about
which stage was tagged.

What is scanned
---------------
Every ``*.yml``/``*.yaml`` under ``.github/workflows``. Physical lines that
end in a backslash are joined first, so a build split over several lines is
read as the single command it is. A logical line that runs ``docker build`` or
``docker buildx build`` against ``docker/Dockerfile`` is classified by its
output:

* a local-output build (``--output ... type=local``, the binary export) must
  pass ``--target export``;
* every other build must pass ``--target runtime``. A missing ``--target``
  fails even though Docker accepts it, because the default is the last stage;
  any other target (``profile``, a typo) fails too.

``docker/Dockerfile`` must define a stage named ``runtime``, or the gate would
bless a target that cannot resolve. ``.github/workflows/release.yml`` must
contain at least one image build, so a workflow rewrite cannot turn the gate
into a vacuous green.

What this gate does not prove
-----------------------------
It reads text: it does not run Docker, pull a base image or inspect the built
image's environment. A target or output flag built from a variable is reported
as missing (unproven), never accepted, and the scan assumes one build command
per logical line. A logical line whose first non-blank character is ``#`` is a
comment and is skipped.

Usage
-----
    python3 .github/scripts/check-docker-build-target.py
    python3 .github/scripts/check-docker-build-target.py --self-test

Exit status is 0 when every product build names the expected stage and 1 when
one does not, when the Dockerfile lacks ``runtime``, when ``release.yml`` has
no image build, or when a workflow cannot be read.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Optional, Tuple

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
DOCKERFILE = ROOT / "docker" / "Dockerfile"

BUILD = re.compile(r"\bdocker\s+(?:buildx\s+)?build\b")
PRODUCT = "docker/Dockerfile"
TARGET = re.compile(r"--target[=\s]+([A-Za-z0-9_.-]+)")
LOCAL_OUTPUT = re.compile(r"--output[=\s]+\S*type=local")
STAGE = re.compile(
    r"^\s*FROM\s+\S+\s+AS\s+([A-Za-z0-9_.-]+)", re.IGNORECASE | re.MULTILINE
)

Finding = Tuple[int, str, Optional[str], Optional[str]]


def logical_lines(text: str) -> list[tuple[int, str]]:
    """Physical lines joined on backslash continuations, with their start line."""
    joined: list[tuple[int, str]] = []
    buffer: list[str] = []
    start: int | None = None
    for number, raw in enumerate(text.splitlines(), 1):
        if start is None:
            start = number
        stripped = raw.rstrip()
        if stripped.endswith("\\"):
            buffer.append(stripped[:-1])
            continue
        buffer.append(stripped)
        joined.append((start, " ".join(part for part in buffer if part)))
        buffer = []
        start = None
    if buffer:
        joined.append((start, " ".join(part for part in buffer if part)))
    return joined


def scan_text(text: str) -> list[Finding]:
    """``(line, kind, target, problem)`` for every product build in *text*.

    ``kind`` is ``image`` for a build that produces an image and ``export`` for
    the local binary export. ``problem`` is ``None`` when the stage is the
    expected one.
    """
    findings: list[Finding] = []
    for line, command in logical_lines(text):
        if command.lstrip().startswith("#"):
            continue
        if not BUILD.search(command) or PRODUCT not in command:
            continue
        match = TARGET.search(command)
        target = match.group(1) if match else None
        if LOCAL_OUTPUT.search(command):
            kind = "export"
            problem = (
                None if target == "export" else "local-output build must pass --target export"
            )
        else:
            kind = "image"
            problem = (
                None if target == "runtime" else "image build must pass --target runtime"
            )
        findings.append((line, kind, target, problem))
    return findings


def describe(name: str, finding: Finding) -> str:
    line, kind, target, problem = finding
    found = f"--target {target}" if target else "no literal --target"
    if problem is None:
        return f"{name}:{line}: {kind} build: {found}"
    return f"{name}:{line}: FAIL: {problem} (found: {found})"


def run() -> int:
    try:
        dockerfile = DOCKERFILE.read_text(encoding="utf-8")
    except OSError as error:
        print(f"FAIL: cannot read {DOCKERFILE}: {error}")
        return 1
    stages = STAGE.findall(dockerfile)
    if "runtime" not in stages:
        listed = ", ".join(stages) if stages else "none"
        print(f"FAIL: docker/Dockerfile defines no stage named 'runtime' (found: {listed})")
        return 1

    workflows = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if not workflows:
        print(f"FAIL: no workflow under {WORKFLOWS}")
        return 1

    failures = 0
    images = 0
    exports = 0
    release_images = 0
    for path in workflows:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            print(f"FAIL: cannot read {path}: {error}")
            failures += 1
            continue
        for finding in scan_text(text):
            print(describe(path.name, finding))
            if finding[1] == "image":
                images += 1
                if path.name == "release.yml":
                    release_images += 1
            else:
                exports += 1
            if finding[3] is not None:
                failures += 1

    if release_images == 0:
        print(
            "FAIL: .github/workflows/release.yml has no product image build; "
            "the gate would pass vacuously"
        )
        failures += 1

    if failures:
        print(f"{failures} failure(s)")
        return 1
    print(f"OK: {images} image build(s) and {exports} export build(s) name the expected stage")
    return 0


SELF_TEST_CASES: list[tuple[str, str, list[Finding]]] = [
    (
        "runtime",
        "docker buildx build --file docker/Dockerfile --target runtime --tag x .",
        [(1, "image", "runtime", None)],
    ),
    (
        "missing-target",
        "docker build --file docker/Dockerfile --tag x .",
        [(1, "image", None, "image build must pass --target runtime")],
    ),
    (
        "profile-target",
        "docker build --file docker/Dockerfile --target profile --tag x .",
        [(1, "image", "profile", "image build must pass --target runtime")],
    ),
    (
        "export-target",
        'docker buildx build --file docker/Dockerfile --target export '
        '--output "type=local,dest=dist" .',
        [(1, "export", "export", None)],
    ),
    (
        "local-output-without-export",
        "docker build --file docker/Dockerfile --target runtime "
        "--output type=local,dest=dist .",
        [(1, "export", "runtime", "local-output build must pass --target export")],
    ),
    (
        "continuation",
        "run: |\n"
        "  docker buildx build --file docker/Dockerfile \\\n"
        "    --platform linux/amd64 \\\n"
        "    --tag x .",
        [(2, "image", None, "image build must pass --target runtime")],
    ),
    ("comment", "# docker build --file docker/Dockerfile .", []),
    ("other-dockerfile", "docker build -f docker/other.Dockerfile --target runtime .", []),
]


def self_test() -> int:
    failures = 0
    for name, text, expected in SELF_TEST_CASES:
        got = scan_text(text)
        if got != expected:
            print(f"self-test {name}: FAIL\n  expected {expected}\n  got      {got}")
            failures += 1
    if failures:
        print(f"self-test: {failures} case(s) failed")
        return 1
    print(f"self-test: {len(SELF_TEST_CASES)} cases OK")
    return 0


def main() -> int:
    summary = (__doc__ or "").strip().splitlines()
    parser = argparse.ArgumentParser(description=summary[0] if summary else None)
    parser.add_argument(
        "--self-test", action="store_true", help="run the detector against embedded fixtures"
    )
    args = parser.parse_args()
    return self_test() if args.self_test else run()


if __name__ == "__main__":
    sys.exit(main())
