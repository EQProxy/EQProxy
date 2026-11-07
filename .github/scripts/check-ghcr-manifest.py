#!/usr/bin/env python3
"""Validate a public GHCR image index and its digest before repointing docs.

The migration record (``docs/release-0.9.2.md``) names the index digest and the
four manifests a release published. This check fetches the same tag from the
public registry **anonymously**, hashes the exact manifest bytes, and validates
the index shape. It is a precondition for changing the GHCR pull example in the
README: a registry tag that moved, a missing architecture or a broken
attestation must fail before the documentation points at it.

Usage:
    # The digest is the value recorded in docs/release-0.9.2.md.
    python3 .github/scripts/check-ghcr-manifest.py \
        --tag 0.9.2 --tag latest \
        --expected-digest sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f

    python3 .github/scripts/check-ghcr-manifest.py \
        --tag 0.9.1 \
        --expected-digest sha256:95170d9a19dc089ccf876a6ae91322e826b64086d7d07763a5462685e2ad775f

    python3 .github/scripts/check-ghcr-manifest.py --self-test

The live mode uses no token, no Docker credential and no repository secret. It
obtains the registry's anonymous pull token for ``repository:eqproxy/eqproxy``
and reads the manifest over HTTPS. Exit status is 0 only when every requested
tag has the expected digest and exactly two Linux platforms plus one attestation
for each. Network or validation failures exit 1.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from typing import Optional
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen

USER_AGENT = "eqproxy-ghcr-check/1"
SHA256_RE = re.compile(r"sha256:[0-9a-f]{64}")
TAG_RE = re.compile(r"[A-Za-z0-9_][A-Za-z0-9._-]{0,127}")
REPOSITORY_RE = re.compile(
    r"[a-z0-9]+(?:[._-][a-z0-9]+)*(?:/[a-z0-9]+(?:[._-][a-z0-9]+)*)*"
)
REGISTRY_RE = re.compile(r"[A-Za-z0-9.-]+(?::[0-9]+)?")

OCI_INDEX = "application/vnd.oci.image.index.v1+json"
DOCKER_LIST = "application/vnd.docker.distribution.manifest.list.v2+json"
INDEX_TYPES = (OCI_INDEX, DOCKER_LIST)
ACCEPT = ", ".join(
    (
        OCI_INDEX,
        DOCKER_LIST,
        "application/vnd.oci.image.manifest.v1+json",
        "application/vnd.docker.distribution.manifest.v2+json",
    )
)
PLATFORMS = ("amd64", "arm64")


class CheckError(RuntimeError):
    """A registry, digest or index-shape check failed."""


@dataclass(frozen=True)
class IndexSummary:
    digest: str
    platforms: dict[str, str]
    attestations: int


def sha256_digest(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def validate_index(
    body: bytes,
    expected_digest: str,
    label: str,
    header_digest: Optional[str] = None,
) -> IndexSummary:
    """Validate the exact index bytes and return what they contain."""
    actual_digest = sha256_digest(body)
    if actual_digest != expected_digest:
        raise CheckError(
            f"{label}: manifest bytes hash to {actual_digest}, "
            f"expected {expected_digest}"
        )
    if header_digest is not None and header_digest != expected_digest:
        raise CheckError(
            f"{label}: Docker-Content-Digest is {header_digest}, "
            f"expected {expected_digest}"
        )

    try:
        document = json.loads(body)
    except json.JSONDecodeError as err:
        raise CheckError(f"{label}: manifest is not JSON: {err}") from None
    if not isinstance(document, dict):
        raise CheckError(f"{label}: manifest root is not an object")
    if document.get("mediaType") not in INDEX_TYPES:
        raise CheckError(
            f"{label}: manifest mediaType is {document.get('mediaType')!r}, "
            "not an OCI/Docker image index"
        )

    entries = document.get("manifests")
    if not isinstance(entries, list):
        raise CheckError(f"{label}: index has no manifests array")

    platforms: dict[str, str] = {}
    attestations: list[tuple[str, str]] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise CheckError(f"{label}: index manifest entry is not an object")
        digest = entry.get("digest")
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            raise CheckError(f"{label}: index entry has an invalid digest: {digest!r}")

        platform = entry.get("platform")
        if not isinstance(platform, dict):
            platform = {}
        annotations = entry.get("annotations")
        if not isinstance(annotations, dict):
            annotations = {}

        arch = platform.get("architecture")
        if platform.get("os") == "linux" and arch in PLATFORMS:
            if arch in platforms:
                raise CheckError(f"{label}: duplicate linux/{arch} manifest")
            platforms[arch] = digest
        elif annotations.get("vnd.docker.reference.type") == "attestation-manifest":
            reference = annotations.get("vnd.docker.reference.digest")
            if not isinstance(reference, str) or not SHA256_RE.fullmatch(reference):
                raise CheckError(
                    f"{label}: attestation has an invalid reference digest: "
                    f"{reference!r}"
                )
            attestations.append((reference, digest))
        else:
            raise CheckError(
                f"{label}: unexpected manifest "
                f"os={platform.get('os')!r} arch={arch!r} digest={digest}"
            )

    missing = [arch for arch in PLATFORMS if arch not in platforms]
    if missing:
        raise CheckError(f"{label}: missing platform(s): {', '.join(missing)}")
    if len(attestations) != len(PLATFORMS):
        raise CheckError(
            f"{label}: expected {len(PLATFORMS)} attestations, "
            f"found {len(attestations)}"
        )

    references = sorted(reference for reference, _ in attestations)
    expected_references = sorted(platforms.values())
    if references != expected_references:
        raise CheckError(
            f"{label}: attestation references {references} do not match "
            f"the platform manifests {expected_references}"
        )

    return IndexSummary(
        digest=actual_digest,
        platforms=platforms,
        attestations=len(attestations),
    )


def anonymous_token(registry: str, repository: str, timeout: float) -> str:
    """Get a pull token without presenting any credential."""
    query = urlencode({"scope": f"repository:{repository}:pull", "service": registry})
    request = Request(
        f"https://{registry}/token?{query}",
        headers={"Accept": "application/json", "User-Agent": USER_AGENT},
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            document = json.load(response)
    except HTTPError as err:
        raise CheckError(
            f"anonymous token request to {registry} failed with HTTP {err.code}"
        ) from None
    except (URLError, OSError, json.JSONDecodeError) as err:
        raise CheckError(f"anonymous token request to {registry} failed: {err}") from None

    token = document.get("token")
    if not isinstance(token, str) or not token:
        raise CheckError(
            f"{registry} did not issue an anonymous pull token; "
            "the package is missing or private"
        )
    return token


def fetch_manifest(
    registry: str,
    repository: str,
    tag: str,
    token: str,
    timeout: float,
) -> tuple[bytes, Optional[str]]:
    """Fetch one manifest and return its exact bytes and reported digest."""
    request = Request(
        f"https://{registry}/v2/{repository}/manifests/{quote(tag, safe='')}",
        headers={
            "Accept": ACCEPT,
            "Authorization": f"Bearer {token}",
            "User-Agent": USER_AGENT,
        },
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            body = response.read()
            header_digest = response.headers.get("Docker-Content-Digest")
    except HTTPError as err:
        detail = err.read(512).decode("utf-8", "replace").strip()
        raise CheckError(
            f"{registry}/{repository}:{tag}: HTTP {err.code}: {detail}"
        ) from None
    except (URLError, OSError) as err:
        raise CheckError(f"{registry}/{repository}:{tag}: network error: {err}") from None
    return body, header_digest


def synthetic_index(include_arm: bool = True) -> tuple[bytes, str]:
    """Build a deterministic index used only by --self-test."""
    amd64 = "sha256:" + "a" * 64
    arm64 = "sha256:" + "b" * 64
    attestation_amd64 = "sha256:" + "c" * 64
    attestation_arm64 = "sha256:" + "d" * 64
    manifests = [
        {
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": amd64,
            "size": 100,
            "platform": {"os": "linux", "architecture": "amd64"},
        },
        {
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": attestation_amd64,
            "size": 100,
            "platform": {"os": "unknown", "architecture": "unknown"},
            "annotations": {
                "vnd.docker.reference.type": "attestation-manifest",
                "vnd.docker.reference.digest": amd64,
            },
        },
    ]
    if include_arm:
        manifests.extend(
            (
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": arm64,
                    "size": 100,
                    "platform": {"os": "linux", "architecture": "arm64"},
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": attestation_arm64,
                    "size": 100,
                    "platform": {"os": "unknown", "architecture": "unknown"},
                    "annotations": {
                        "vnd.docker.reference.type": "attestation-manifest",
                        "vnd.docker.reference.digest": arm64,
                    },
                },
            )
        )
    document = {
        "schemaVersion": 2,
        "mediaType": OCI_INDEX,
        "manifests": manifests,
    }
    body = json.dumps(document, separators=(",", ":"), sort_keys=True).encode()
    return body, sha256_digest(body)


def expect_rejected(label: str, body: bytes, expected_digest: str, needle: str) -> None:
    try:
        validate_index(body, expected_digest, label)
    except CheckError as err:
        if needle not in str(err):
            raise CheckError(
                f"self-test: {label} failed for the wrong reason: {err}"
            ) from None
        return
    raise CheckError(f"self-test: {label} was accepted")


def self_test() -> None:
    body, digest = synthetic_index()
    summary = validate_index(body, digest, "synthetic")
    if summary.platforms != {
        "amd64": "sha256:" + "a" * 64,
        "arm64": "sha256:" + "b" * 64,
    }:
        raise CheckError("self-test: synthetic platform map is wrong")
    if summary.attestations != 2:
        raise CheckError("self-test: synthetic attestation count is wrong")

    expect_rejected("digest mismatch", body, "sha256:" + "0" * 64, "hash to")
    missing_arm, missing_arm_digest = synthetic_index(include_arm=False)
    expect_rejected("missing arm64", missing_arm, missing_arm_digest, "arm64")

    document = json.loads(body)
    document["manifests"][1]["annotations"]["vnd.docker.reference.digest"] = (
        "sha256:" + "e" * 64
    )
    mutated = json.dumps(document, separators=(",", ":"), sort_keys=True).encode()
    expect_rejected(
        "wrong attestation reference",
        mutated,
        sha256_digest(mutated),
        "attestation references",
    )
    print(
        "self-test OK: accepted a complete index and rejected digest, "
        "platform and attestation mutations"
    )


def positive_float(value: str) -> float:
    try:
        number = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be a number") from None
    if number <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return number


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Verify a public GHCR index digest and its linux/amd64, linux/arm64 "
            "and attestation manifests before repointing the README."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Example:\n"
            "  python3 .github/scripts/check-ghcr-manifest.py \\\n"
            "    --tag 0.9.2 --tag latest \\\n"
            "    --expected-digest sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f"
        ),
    )
    parser.add_argument("--registry", default="ghcr.io")
    parser.add_argument("--repository", default="eqproxy/eqproxy")
    parser.add_argument(
        "--tag",
        action="append",
        default=[],
        metavar="TAG",
        help="tag to verify; repeat for several tags (for example 0.9.2 and latest)",
    )
    parser.add_argument(
        "--expected-digest",
        help="sha256:<64 hex> recorded in the release document",
    )
    parser.add_argument("--timeout", type=positive_float, default=20.0)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="test the validator with synthetic manifests; no network access",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        try:
            self_test()
        except CheckError as err:
            print(f"error: {err}", file=sys.stderr)
            return 1
        return 0

    if not args.tag:
        print("error: at least one --tag is required (or use --self-test)", file=sys.stderr)
        return 2
    if not args.expected_digest or not SHA256_RE.fullmatch(args.expected_digest):
        print(
            "error: --expected-digest must be sha256:<64 lowercase hex>",
            file=sys.stderr,
        )
        return 2
    if not REGISTRY_RE.fullmatch(args.registry):
        print("error: --registry is not a valid host", file=sys.stderr)
        return 2
    if not REPOSITORY_RE.fullmatch(args.repository):
        print("error: --repository is not a valid lowercase repository", file=sys.stderr)
        return 2
    for tag in args.tag:
        if not TAG_RE.fullmatch(tag):
            print(f"error: invalid tag: {tag!r}", file=sys.stderr)
            return 2

    try:
        token = anonymous_token(args.registry, args.repository, args.timeout)
        for tag in args.tag:
            body, header_digest = fetch_manifest(
                args.registry, args.repository, tag, token, args.timeout
            )
            summary = validate_index(
                body,
                args.expected_digest,
                f"{args.registry}/{args.repository}:{tag}",
                header_digest,
            )
            print(f"{args.registry}/{args.repository}:{tag}")
            print(f"  digest: {summary.digest}")
            for arch in PLATFORMS:
                print(f"  linux/{arch}: {summary.platforms[arch]}")
            print(f"  attestations: {summary.attestations}")
    except CheckError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
