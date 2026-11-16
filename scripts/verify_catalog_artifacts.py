"""Verify that every published catalog artifact is REAL: reachable, the
exact recorded size, and the exact recorded SHA-256.

Maintainer tool (not a test — tests never touch the network per
testing-strategy.md). Run against a live catalog API:

    python scripts/verify_catalog_artifacts.py https://catalog.example
    python scripts/verify_catalog_artifacts.py http://127.0.0.1:8000

The catalog base URL may be plain HTTP for local development (loopback
carve-out, security-model.md §3.1); every ARTIFACT URL is enforced
HTTPS-only, as published data must be.

What it checks per artifact:
  1. The artifact URL is HTTPS (no credentials).
  2. The vendor responds 200 and the streamed body's SHA-256 equals the
     catalog record.
  3. The streamed byte count equals size_bytes.

It downloads for verification but never writes or executes anything.
Exit code 0 = every artifact verified; 1 = at least one failure.
"""
from __future__ import annotations

import hashlib
import json
import sys
import urllib.request
from urllib.parse import urlsplit

CHUNK = 1 << 16
TIMEOUT = 120  # seconds; some vendor CDNs are slow on first byte


def fail(message: str) -> None:
    print(f"  FAIL  {message}")
    failures.append(message)


failures: list[str] = []


def fetch_json(url: str) -> object:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
        if response.status != 200:
            raise RuntimeError(f"HTTP {response.status} for {url}")
        return json.loads(response.read().decode("utf-8"))


def verify_artifact(package_id: str, version: str, artifact: dict) -> None:
    url = artifact.get("url", "")
    expected_sha = (artifact.get("sha256") or "").lower()
    expected_size = artifact.get("size_bytes")

    print(f"[{package_id} {version}] {url}")

    scheme = urlsplit(url).scheme
    if scheme != "https":
        fail(f"artifact URL must be HTTPS, got {scheme}:")
        return
    if "@" in urlsplit(url).netloc:
        fail("artifact URL contains credentials")
        return
    if len(expected_sha) != 64:
        fail(f"catalog sha256 is not 64 hex chars: {expected_sha!r}")
        return

    digest = hashlib.sha256()
    size = 0
    request = urllib.request.Request(url, headers={"Accept": "*/*"})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
            if response.status != 200:
                fail(f"vendor returned HTTP {response.status}")
                return
            while True:
                chunk = response.read(CHUNK)
                if not chunk:
                    break
                digest.update(chunk)
                size += len(chunk)
    except Exception as exc:  # noqa: BLE001 — report any transport failure
        fail(f"download failed: {exc}")
        return

    actual_sha = digest.hexdigest()
    if actual_sha != expected_sha:
        fail(f"sha256 mismatch: catalog={expected_sha} actual={actual_sha}")
    if expected_size is not None and size != expected_size:
        fail(f"size mismatch: catalog={expected_size} actual={size}")
    if actual_sha == expected_sha and (expected_size is None or size == expected_size):
        print(f"  ok    sha256 + size verified ({size} bytes)")


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    base = sys.argv[1].rstrip("/")
    if not base.startswith(("http://", "https://")):
        print("base URL must start with http:// or https://")
        return 2

    print(f"catalog API: {base}")
    packages = fetch_json(f"{base}/api/v1/catalog/packages?page_size=100")
    results = packages.get("results", [])
    print(f"published packages: {packages.get('count', len(results))}")
    if not results:
        print("nothing to verify — seed the catalog first "
              "(python manage.py seed_real_catalog)")
        return 1

    verified = 0
    for summary in results:
        package_id = summary["id"]
        manifest = fetch_json(f"{base}/api/v1/catalog/packages/{package_id}/manifest")
        for version in manifest.get("versions", []):
            for artifact in version.get("artifacts", []):
                verify_artifact(package_id, version["version"], artifact)
                verified += 1

    print(f"\nverified {verified} artifact(s)")
    if failures:
        print(f"{len(failures)} FAILURE(S)")
        return 1
    print("all artifacts are real: HTTPS, size, and SHA-256 verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
