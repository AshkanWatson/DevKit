/// Embedded Phase 1 fixture catalog (DESIGN.md §11: "Explore list/search/
/// category filters using local fixture catalog").
///
/// These documents mirror `schemas/examples/` and the committed Rust test
/// fixtures (crates/devkit-catalog/tests/fixtures). Two additional packages
/// (git, vscode) make the profile references resolvable inside the fake
/// catalog. All URLs use example.invalid domains — nothing here is ever
/// downloaded; Phase 1 performs no network access.
library;

const String flutterPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.framework.flutter",
  "name": "Flutter",
  "description": "Google's UI toolkit for building natively compiled applications for mobile, web, and desktop from a single codebase.",
  "publisher": "Google",
  "homepage": "https://flutter.dev/",
  "category": "framework",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.27.1",
      "channel": "stable",
      "release_date": "2024-12-11",
      "release_notes_url": "https://docs.flutter.dev/release-notes/release-notes-3.27.0",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://storage.example.invalid/flutter/flutter_windows_3.27.1-stable.zip",
          "sha256": "1111111111111111111111111111111111111111111111111111111111111111",
          "size_bytes": 1100000000, "archive_root": "flutter",
          "executable_relative_path": "bin/flutter.bat"
        },
        {
          "os": "macos", "architecture": "arm64", "installer": "archive",
          "url": "https://storage.example.invalid/flutter/flutter_macos_arm64_3.27.1-stable.zip",
          "sha256": "2222222222222222222222222222222222222222222222222222222222222222",
          "size_bytes": 1050000000, "archive_root": "flutter",
          "executable_relative_path": "bin/flutter"
        }
      ]
    }
  ],
  "license_expression": "BSD-3-Clause",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String pythonPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.python",
  "name": "Python",
  "description": "Python programming language runtime.",
  "publisher": "Python Software Foundation",
  "homepage": "https://www.python.org/",
  "category": "language",
  "supported_platforms": ["windows-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.12.7",
      "channel": "stable",
      "release_date": "2024-10-01",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://example.invalid/python-3.12.7-amd64.exe",
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "size_bytes": 30000000
        }
      ]
    },
    {
      "version": "3.13.1",
      "channel": "beta",
      "release_date": "2025-12-03",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://example.invalid/python-3.13.1-amd64.exe",
          "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "size_bytes": 31000000
        }
      ]
    }
  ],
  "license_expression": "PSF-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String gitPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.utility.git",
  "name": "Git",
  "description": "Distributed version control system.",
  "publisher": "Git SCM",
  "homepage": "https://git-scm.com/",
  "category": "utility",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "2.51.0",
      "channel": "stable",
      "release_date": "2026-08-20",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://example.invalid/git-2.51.0-64-bit.exe",
          "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
          "size_bytes": 65000000
        }
      ]
    }
  ],
  "license_expression": "GPL-2.0-only",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String vscodePackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.ide.vscode",
  "name": "Visual Studio Code",
  "description": "Source code editor with built-in support for development tooling.",
  "publisher": "Microsoft",
  "homepage": "https://code.visualstudio.com/",
  "category": "ide",
  "supported_platforms": ["windows-x64", "windows-arm64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "latest",
  "versions": [
    {
      "version": "1.98.0",
      "channel": "stable",
      "release_date": "2026-03-05",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://example.invalid/vscode-1.98.0-x64.exe",
          "sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          "size_bytes": 95000000
        }
      ]
    },
    {
      "version": "1.99.0-insider",
      "channel": "dev",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://example.invalid/vscode-1.99.0-insider-x64.exe",
          "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
          "size_bytes": 96000000
        }
      ]
    }
  ],
  "license_expression": "MIT",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String flutterProfileJson = r'''
{
  "schema_version": 1,
  "id": "devkit.profile.flutter-developer",
  "name": "Flutter Developer",
  "description": "Everything needed to start building Flutter applications: the Flutter SDK, Git, and a recommended editor.",
  "maintainer": "DevKit Catalog Team",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "packages": [
    {
      "package_id": "devkit.framework.flutter",
      "required": true,
      "version_policy": "stable",
      "reason": "Primary SDK for Flutter application development."
    },
    {
      "package_id": "devkit.utility.git",
      "required": true,
      "version_policy": "stable",
      "reason": "Flutter tooling requires Git for dependency management."
    },
    {
      "package_id": "devkit.ide.vscode",
      "required": false,
      "version_policy": "latest",
      "reason": "Recommended editor with the official Flutter extension."
    }
  ]
}
''';

const String pythonProfileJson = r'''
{
  "schema_version": 1,
  "id": "devkit.profile.python-developer",
  "name": "Python Developer",
  "description": "A productive Python setup: the language runtime, Git, and a recommended editor.",
  "maintainer": "DevKit Catalog Team",
  "supported_platforms": ["windows-x64", "macos-arm64", "linux-x64"],
  "packages": [
    {
      "package_id": "devkit.lang.python",
      "required": true,
      "version_policy": "exact",
      "version": "3.12.7",
      "reason": "Pinned runtime for reproducible environments."
    },
    {
      "package_id": "devkit.utility.git",
      "required": true,
      "version_policy": "stable",
      "reason": "Package management and version control."
    },
    {
      "package_id": "devkit.ide.vscode",
      "required": false,
      "version_policy": "channel",
      "channel": "stable",
      "reason": "Recommended editor with Python tooling."
    }
  ]
}
''';

const String dockerPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.tool.docker",
  "name": "Docker Engine",
  "description": "Container runtime for building and running containerized workloads.",
  "publisher": "Docker Inc.",
  "homepage": "https://www.docker.com/",
  "category": "utility",
  "supported_platforms": ["linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "27.3.1",
      "channel": "stable",
      "release_date": "2026-09-20",
      "artifacts": [
        {
          "os": "linux", "architecture": "x64", "installer": "archive",
          "url": "https://example.invalid/docker-27.3.1-x64.tgz",
          "sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
          "size_bytes": 70000000
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

/// All embedded fixture documents: five package manifests, two profiles.
const List<String> fixturePackageDocuments = [
  flutterPackageJson,
  pythonPackageJson,
  gitPackageJson,
  vscodePackageJson,
  dockerPackageJson,
];

const List<String> fixtureProfileDocuments = [
  flutterProfileJson,
  pythonProfileJson,
];
