"""Seed the catalog with REAL packages, versions, and artifacts.

Maintenance tool (not a test): inserts well-known developer packages with
their real vendor download URLs and the exact SHA-256/size measured from
the actual published files (see scripts/verify_catalog_artifacts.py, which
re-verifies every URL, size, and hash against the vendor servers).

Facts recorded here were measured from the vendor releases on 2026-11-15
(HTTPS GET + SHA-256 over the full body). Flutter (P5-09) is the exception:
its URL, SHA-256 and channel come from Google's own release manifest
(`releases_<os>.json`), and its size from the authoritative HTTPS
`Content-Length` HEAD response on 2026-11-28 — no value is guessed. If a
vendor replaces a file, `python manage.py seed_real_catalog` alone is not
enough: re-measure and update this file, then run
scripts/verify_catalog_artifacts.py.

This command never downloads anything — it writes metadata only. Tests
stay offline (testing-strategy.md); the real-URL verification is the
script's job, run by a maintainer.
"""
from __future__ import annotations

import datetime

from django.core.management.base import BaseCommand
from django.db import transaction

from catalog import publishing
from catalog.models import Artifact, Package, PackagePlatform, PackageVersion

# Real, version-pinned vendor URLs. Versioned release links are used on
# purpose (never "latest" redirects) so the recorded hash stays meaningful.
SEED_PACKAGES: list[dict] = [
    {
        "id": "devkit.lang.python",
        "name": "Python",
        "description": (
            "The Python programming language (Windows embeddable package: "
            "a minimal interpreter layout suitable for managed installs)."
        ),
        "publisher": "Python Software Foundation",
        "homepage": "https://www.python.org/",
        "category": "language",
        "license_expression": "PSF-2.0",
        "maintainer": "devkit-maintainers",
        "platforms": ["windows-x64"],
        "versions": [
            {
                "version": "3.12.8",
                "channel": "stable",
                "release_date": datetime.date(2024, 12, 4),
                "artifacts": [
                    {
                        "os": "windows",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://www.python.org/ftp/python/3.12.8/"
                            "python-3.12.8-embed-amd64.zip"
                        ),
                        "sha256": (
                            "8d3f33be9eb810f23c102f08475af2854e50484b8e4e"
                            "06275e937be61ce3d2fb"
                        ),
                        "size_bytes": 11094114,
                        "executable_relative_path": "python.exe",
                    },
                ],
            },
        ],
    },
    {
        # P5-09 (ADR 0009): Flutter is the first official-provider package.
        # Values below are copied from the verified 2026-11-28 Flutter
        # release manifests (stable 3.50.1) and HTTPS HEAD lengths. The
        # Rust `FlutterSource` resolves these live at install time; this
        # row exists so browsing/details show real metadata.
        "id": "devkit.framework.flutter",
        "name": "Flutter",
        "description": (
            "Google's UI toolkit for building natively compiled applications "
            "for mobile, web, and desktop from a single codebase."
        ),
        "publisher": "Google",
        "homepage": "https://flutter.dev/",
        "category": "framework",
        "license_expression": "BSD-3-Clause",
        "maintainer": "devkit-maintainers",
        "platforms": [
            "windows-x64",
            "macos-x64",
            "macos-arm64",
            "linux-x64",
        ],
        "versions": [
            {
                "version": "3.50.1",
                "channel": "stable",
                "release_date": datetime.date(2026, 11, 20),
                "artifacts": [
                    {
                        "os": "windows",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://storage.googleapis.com/"
                            "flutter_infra_release/releases/stable/windows/"
                            "flutter_windows_3.50.1-stable.zip"
                        ),
                        "sha256": (
                            "33c1e3d4dd08f1136ddb9c7976cfd2b0ab20296f1565"
                            "b78ba176a4cbb4e140eb"
                        ),
                        "size_bytes": 1869410092,
                        "executable_relative_path": "flutter/bin/flutter.bat",
                    },
                    {
                        "os": "macos",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://storage.googleapis.com/"
                            "flutter_infra_release/releases/stable/macos/"
                            "flutter_macos_3.50.1-stable.zip"
                        ),
                        "sha256": (
                            "5c3d3bbabc8edd58eb98605047c59c31a82895cfe7d"
                            "119318d0f62cdbd5d47ed"
                        ),
                        "size_bytes": 2141226529,
                        "executable_relative_path": "flutter/bin/flutter",
                    },
                    {
                        "os": "macos",
                        "architecture": "arm64",
                        "installer": "archive",
                        "url": (
                            "https://storage.googleapis.com/"
                            "flutter_infra_release/releases/stable/macos/"
                            "flutter_macos_arm64_3.50.1-stable.zip"
                        ),
                        "sha256": (
                            "872883d49965e0cc1d8e477d2d87f2f45930d13f94"
                            "b34889e53783586f04c715"
                        ),
                        "size_bytes": 2145372380,
                        "executable_relative_path": "flutter/bin/flutter",
                    },
                    {
                        "os": "linux",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://storage.googleapis.com/"
                            "flutter_infra_release/releases/stable/linux/"
                            "flutter_linux_3.50.1-stable.tar.xz"
                        ),
                        "sha256": (
                            "4ad17fc8462fbcef2ba19ff4a78c0a16b7147fde1a"
                            "dd8578e267ea62cdb92678"
                        ),
                        "size_bytes": 1518418752,
                        "executable_relative_path": "flutter/bin/flutter",
                    },
                ],
            },
        ],
    },
    {
        "id": "devkit.utility.ripgrep",
        "name": "ripgrep",
        "description": (
            " recursively searches directories for a regex pattern (rg)."
        ),
        "publisher": "BurntSushi",
        "homepage": "https://github.com/BurntSushi/ripgrep",
        "category": "utility",
        "license_expression": "MIT",
        "maintainer": "devkit-maintainers",
        "platforms": ["windows-x64"],
        "versions": [
            {
                "version": "14.1.1",
                "channel": "stable",
                "release_date": datetime.date(2025, 1, 9),
                "artifacts": [
                    {
                        "os": "windows",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://github.com/BurntSushi/ripgrep/releases/"
                            "download/14.1.1/"
                            "ripgrep-14.1.1-x86_64-pc-windows-msvc.zip"
                        ),
                        "sha256": (
                            "d0f534024c42afd6cb4d38907c25cd2b249b79bbe6cc1db"
                            "ee8e3e37c2b6e25a1"
                        ),
                        "size_bytes": 2058893,
                        "executable_relative_path": (
                            "ripgrep-14.1.1-x86_64-pc-windows-msvc/rg.exe"
                        ),
                    },
                ],
            },
        ],
    },
    {
        "id": "devkit.utility.fd",
        "name": "fd",
        "description": "A simple, fast and user-friendly alternative to find.",
        "publisher": "sharkdp",
        "homepage": "https://github.com/sharkdp/fd",
        "category": "utility",
        "license_expression": "MIT OR Apache-2.0",
        "maintainer": "devkit-maintainers",
        "platforms": ["windows-x64"],
        "versions": [
            {
                "version": "10.2.0",
                "channel": "stable",
                "release_date": datetime.date(2024, 10, 13),
                "artifacts": [
                    {
                        "os": "windows",
                        "architecture": "x64",
                        "installer": "archive",
                        "url": (
                            "https://github.com/sharkdp/fd/releases/download/"
                            "v10.2.0/fd-v10.2.0-x86_64-pc-windows-msvc.zip"
                        ),
                        "sha256": (
                            "92ac9e6b0a0c6ecdab638ffe210dc786403fff4c663736"
                            "04cf70df27be45e4fe"
                        ),
                        "size_bytes": 1315212,
                        "executable_relative_path": (
                            "fd-v10.2.0-x86_64-pc-windows-msvc/fd.exe"
                        ),
                    },
                ],
            },
        ],
    },
]


class Command(BaseCommand):
    help = (
        "Seed (upsert) the catalog with real packages, versions, and "
        "artifacts, then publish them through the normal validation gate."
    )

    def handle(self, *args, **options) -> None:
        created_packages = 0
        updated_packages = 0
        quiet = options.get("verbosity", 1) == 0
        with transaction.atomic():
            for data in SEED_PACKAGES:
                package, created = Package.objects.update_or_create(
                    id=data["id"],
                    defaults={
                        "name": data["name"],
                        "description": data["description"].strip(),
                        "publisher": data["publisher"],
                        "homepage": data["homepage"],
                        "category": data["category"],
                        "default_version_policy": "stable",
                        "license_expression": data["license_expression"],
                        "maintainer": data["maintainer"],
                    },
                )
                created_packages += int(created)
                updated_packages += int(not created)

                for platform in data["platforms"]:
                    PackagePlatform.objects.update_or_create(
                        package=package, platform=platform
                    )

                for version_data in data["versions"]:
                    version, _ = PackageVersion.objects.update_or_create(
                        package=package,
                        version=version_data["version"],
                        defaults={
                            "channel": version_data["channel"],
                            "release_date": version_data["release_date"],
                        },
                    )
                    for artifact_data in version_data["artifacts"]:
                        Artifact.objects.update_or_create(
                            package_version=version,
                            os=artifact_data["os"],
                            architecture=artifact_data["architecture"],
                            installer=artifact_data["installer"],
                            defaults={
                                "url": artifact_data["url"],
                                "sha256": artifact_data["sha256"],
                                "size_bytes": artifact_data["size_bytes"],
                                "executable_relative_path": artifact_data[
                                    "executable_relative_path"
                                ],
                            },
                        )

                # Publish through the same validation gate the admin uses
                # (HTTPS-only URLs, well-formed SHA-256, required metadata).
                publishing.publish_package(package)
                if not quiet:
                    self.stdout.write(
                        f"seeded + published {package.id} "
                        f"({'created' if created else 'updated'})"
                    )
        if quiet:
            return
        self.stdout.write(
            self.style.SUCCESS(
                f"done: {created_packages} created, "
                f"{updated_packages} updated, all published"
            )
        )
