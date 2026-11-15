/// Embedded Phase 1 fixture catalog (DESIGN.md §11: "Explore list/search/
/// category filters using local fixture catalog").
///
/// These documents mirror `schemas/examples/` and the committed Rust test
/// fixtures (crates/devkit-catalog/tests/fixtures). Additional packages make
/// the profile references resolvable inside the fake catalog and cover
/// every catalog category (languages, frameworks, SDKs, IDEs, build tools,
/// databases, utilities, AI tools) so category filtering is meaningful
/// across the full range. All URLs use example.invalid domains — nothing
/// here is ever downloaded; Phase 1 performs no network access.
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

const String nodejsPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.nodejs",
  "name": "Node.js",
  "description": "JavaScript runtime built on Chrome's V8 engine for building server-side and tooling applications.",
  "publisher": "OpenJS Foundation",
  "homepage": "https://nodejs.org/",
  "category": "language",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "26.13.1",
      "channel": "stable",
      "release_date": "2026-10-13",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://nodejs.example.invalid/node-v26.13.1-win-x64.zip",
          "sha256": "1010101010101010101010101010101010101010101010101010101010101010",
          "size_bytes": 36000000, "archive_root": "node",
          "executable_relative_path": "bin/node.exe"
        },
        {
          "os": "macos", "architecture": "arm64", "installer": "archive",
          "url": "https://nodejs.example.invalid/node-v26.13.1-darwin-arm64.tar.gz",
          "sha256": "1011111111111111111111111111111111111111111111111111111111111111",
          "size_bytes": 34000000, "archive_root": "node",
          "executable_relative_path": "bin/node"
        }
      ]
    }
  ],
  "license_expression": "MIT",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String goPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.go",
  "name": "Go",
  "description": "The Go programming language toolchain: compiler, standard library, and go command.",
  "publisher": "Google",
  "homepage": "https://go.dev/",
  "category": "language",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "1.23.4",
      "channel": "stable",
      "release_date": "2024-12-03",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://go.example.invalid/go1.23.4.windows-amd64.zip",
          "sha256": "1111111111111111111111111111111111111111111111111111111111111111",
          "size_bytes": 80000000, "archive_root": "go",
          "executable_relative_path": "bin/go.exe"
        }
      ]
    }
  ],
  "license_expression": "BSD-3-Clause",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String rustPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.rust",
  "name": "Rust (toolchain)",
  "description": "Systems programming language toolchain: rustc compiler, Cargo build system, and standard library.",
  "publisher": "Rust Project",
  "homepage": "https://www.rust-lang.org/",
  "category": "language",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "1.97.1",
      "channel": "stable",
      "release_date": "2026-10-16",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://rust.example.invalid/rust-1.97.1-x86_64-pc-windows-msvc.exe",
          "sha256": "1212121212121212121212121212121212121212121212121212121212121212",
          "size_bytes": 250000000
        }
      ]
    }
  ],
  "license_expression": "MIT OR Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String jdkPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.java",
  "name": "Eclipse Temurin JDK",
  "description": "OpenJDK distribution from the Eclipse Temurin project: Java compiler and runtime for LTS release 21.",
  "publisher": "Eclipse Foundation",
  "homepage": "https://adoptium.net/",
  "category": "language",
  "supported_platforms": ["windows-x64", "windows-arm64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "21.0.5",
      "channel": "stable",
      "release_date": "2024-10-15",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://adoptium.example.invalid/temurin-21.0.5-windows-x64.zip",
          "sha256": "1313131313131313131313131313131313131313131313131313131313131313",
          "size_bytes": 195000000, "archive_root": "jdk",
          "executable_relative_path": "bin/java.exe"
        }
      ]
    }
  ],
  "license_expression": "GPL-2.0-only WITH Classpath-exception-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String kotlinPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.lang.kotlin",
  "name": "Kotlin",
  "description": "Kotlin compiler and command-line tools for JVM, multiplatform, and native targets.",
  "publisher": "JetBrains",
  "homepage": "https://kotlinlang.org/",
  "category": "language",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "2.1.0",
      "channel": "stable",
      "release_date": "2024-11-19",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://kotlin.example.invalid/kotlin-compiler-2.1.0.zip",
          "sha256": "1414141414141414141414141414141414141414141414141414141414141414",
          "size_bytes": 90000000, "archive_root": "kotlinc",
          "executable_relative_path": "bin/kotlin.bat"
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String dotnetPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.sdk.dotnet",
  "name": ".NET SDK",
  "description": "The .NET SDK: runtime, compiler, and tooling for building C#/F# applications and libraries.",
  "publisher": "Microsoft",
  "homepage": "https://dotnet.microsoft.com/",
  "category": "sdk",
  "supported_platforms": ["windows-x64", "windows-arm64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "9.0.100",
      "channel": "stable",
      "release_date": "2024-11-12",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://dotnet.example.invalid/dotnet-sdk-9.0.100-win-x64.zip",
          "sha256": "1515151515151515151515151515151515151515151515151515151515151515",
          "size_bytes": 230000000, "archive_root": "dotnet",
          "executable_relative_path": "dotnet.exe"
        }
      ]
    }
  ],
  "license_expression": "MIT",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String androidToolsPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.sdk.android-tools",
  "name": "Android SDK Command-Line Tools",
  "description": "Official Android SDK command-line tools: sdkmanager, avdmanager, and build tooling for Android development.",
  "publisher": "Google",
  "homepage": "https://developer.android.com/studio",
  "category": "sdk",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "latest",
  "versions": [
    {
      "version": "12.0",
      "channel": "stable",
      "release_date": "2024-09-10",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://android.example.invalid/commandlinetools-win-12.0.zip",
          "sha256": "1616161616161616161616161616161616161616161616161616161616161616",
          "size_bytes": 140000000, "archive_root": "cmdline-tools",
          "executable_relative_path": "bin/sdkmanager.bat"
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String springBootPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.framework.spring-boot",
  "name": "Spring Boot",
  "description": "Application framework for production-ready JVM microservices and web applications, with the Spring Boot CLI.",
  "publisher": "Broadcom (Spring)",
  "homepage": "https://spring.io/projects/spring-boot",
  "category": "framework",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.4.1",
      "channel": "stable",
      "release_date": "2024-12-19",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://spring.example.invalid/spring-boot-cli-3.4.1-bin.zip",
          "sha256": "1717171717171717171717171717171717171717171717171717171717171717",
          "size_bytes": 60000000, "archive_root": "spring-boot",
          "executable_relative_path": "bin/spring.bat"
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String nextjsPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.framework.nextjs",
  "name": "Next.js",
  "description": "React framework for building full-stack web applications with server-side rendering and static generation.",
  "publisher": "Vercel",
  "homepage": "https://nextjs.org/",
  "category": "framework",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "latest",
  "versions": [
    {
      "version": "15.1.0",
      "channel": "stable",
      "release_date": "2024-12-05",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://nextjs.example.invalid/nextjs-15.1.0-windows-x64.zip",
          "sha256": "1818181818181818181818181818181818181818181818181818181818181818",
          "size_bytes": 45000000, "archive_root": "nextjs"
        }
      ]
    }
  ],
  "license_expression": "MIT",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String androidStudioPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.ide.android-studio",
  "name": "Android Studio",
  "description": "Official IDE for Android development, built on IntelliJ platform with Android SDK integration.",
  "publisher": "Google",
  "homepage": "https://developer.android.com/studio",
  "category": "ide",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "2024.2.2",
      "channel": "stable",
      "release_date": "2024-12-02",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://android.example.invalid/android-studio-2024.2.2-windows.exe",
          "sha256": "1919191919191919191919191919191919191919191919191919191919191919",
          "size_bytes": 1100000000
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String intellijPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.ide.intellij",
  "name": "IntelliJ IDEA Community",
  "description": "Open-source IntelliJ IDEA Community Edition: JVM and multi-language IDE with Kotlin and Java tooling.",
  "publisher": "JetBrains",
  "homepage": "https://www.jetbrains.com/idea/",
  "category": "ide",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "2024.3",
      "channel": "stable",
      "release_date": "2024-11-13",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://jetbrains.example.invalid/ideaIC-2024.3.win.exe",
          "sha256": "1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a",
          "size_bytes": 850000000
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String gradlePackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.build.gradle",
  "name": "Gradle",
  "description": "Build automation tool for JVM, Android, and native projects with incremental builds and caching.",
  "publisher": "Gradle Inc.",
  "homepage": "https://gradle.org/",
  "category": "build_tool",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "8.11.1",
      "channel": "stable",
      "release_date": "2024-11-19",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://gradle.example.invalid/gradle-8.11.1-bin.zip",
          "sha256": "1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b",
          "size_bytes": 135000000, "archive_root": "gradle",
          "executable_relative_path": "bin/gradle.bat"
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String mavenPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.build.maven",
  "name": "Apache Maven",
  "description": "Build and dependency management tool for JVM projects with a rich plugin ecosystem.",
  "publisher": "Apache Software Foundation",
  "homepage": "https://maven.apache.org/",
  "category": "build_tool",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.9.9",
      "channel": "stable",
      "release_date": "2024-11-04",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://maven.example.invalid/maven-3.9.9-bin.zip",
          "sha256": "1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c",
          "size_bytes": 9500000, "archive_root": "maven",
          "executable_relative_path": "bin/mvn.cmd"
        }
      ]
    }
  ],
  "license_expression": "Apache-2.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String cmakePackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.build.cmake",
  "name": "CMake",
  "description": "Cross-platform build system generator for native projects (C/C++/Rust tooling ecosystems).",
  "publisher": "Kitware",
  "homepage": "https://cmake.org/",
  "category": "build_tool",
  "supported_platforms": ["windows-x64", "windows-arm64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.31.2",
      "channel": "stable",
      "release_date": "2024-12-09",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://cmake.example.invalid/cmake-3.31.2-windows-x86_64.zip",
          "sha256": "1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d",
          "size_bytes": 48000000, "archive_root": "cmake",
          "executable_relative_path": "bin/cmake.exe"
        }
      ]
    }
  ],
  "license_expression": "BSD-3-Clause",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String postgresqlPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.database.postgresql",
  "name": "PostgreSQL",
  "description": "Advanced open-source relational database with strong SQL standards compliance.",
  "publisher": "PostgreSQL Global Development Group",
  "homepage": "https://www.postgresql.org/",
  "category": "database",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "17.2",
      "channel": "stable",
      "release_date": "2024-11-14",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://postgresql.example.invalid/postgresql-17.2-1-windows-x64.exe",
          "sha256": "1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e",
          "size_bytes": 350000000
        }
      ]
    }
  ],
  "license_expression": "PostgreSQL",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String redisPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.database.redis",
  "name": "Redis",
  "description": "In-memory data structure server used as a database, cache, and message broker.",
  "publisher": "Redis Ltd.",
  "homepage": "https://redis.io/",
  "category": "database",
  "supported_platforms": ["windows-x64", "linux-x64", "linux-arm64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "7.4.1",
      "channel": "stable",
      "release_date": "2024-09-09",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://redis.example.invalid/redis-7.4.1-windows-x64.zip",
          "sha256": "1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
          "size_bytes": 12000000, "archive_root": "redis"
        }
      ]
    }
  ],
  "license_expression": "RSAL-2.0 OR SSPL-1.0",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String sqlitePackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.database.sqlite",
  "name": "SQLite",
  "description": "Self-contained, serverless SQL database engine with its command-line shell and tools bundle.",
  "publisher": "SQLite Consortium",
  "homepage": "https://www.sqlite.org/",
  "category": "database",
  "supported_platforms": ["windows-x64", "macos-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "stable",
  "versions": [
    {
      "version": "3.47.2",
      "channel": "stable",
      "release_date": "2024-12-10",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "archive",
          "url": "https://sqlite.example.invalid/sqlite-tools-3.47.2-win-x64.zip",
          "sha256": "2020202020202020202020202020202020202020202020202020202020202020",
          "size_bytes": 2500000, "archive_root": "sqlite-tools",
          "executable_relative_path": "sqlite3.exe"
        }
      ]
    }
  ],
  "license_expression": "LicenseRef-SQLite-blessing",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

const String ollamaPackageJson = r'''
{
  "schema_version": 1,
  "id": "devkit.ai.ollama",
  "name": "Ollama",
  "description": "Local large-language model runner: pull, run, and serve open models entirely on your machine.",
  "publisher": "Ollama",
  "homepage": "https://ollama.com/",
  "category": "ai_tool",
  "supported_platforms": ["windows-x64", "macos-arm64", "linux-x64"],
  "default_version_policy": "latest",
  "versions": [
    {
      "version": "0.5.4",
      "channel": "stable",
      "release_date": "2024-12-12",
      "artifacts": [
        {
          "os": "windows", "architecture": "x64", "installer": "exe",
          "url": "https://ollama.example.invalid/OllamaSetup-0.5.4.exe",
          "sha256": "2121212121212121212121212121212121212121212121212121212121212121",
          "size_bytes": 900000000
        }
      ]
    }
  ],
  "license_expression": "MIT",
  "maintainer": "DevKit Catalog Team",
  "updated_at": "2026-11-12T00:00:00Z"
}
''';

/// All embedded fixture documents: twenty-three package manifests, two
/// profiles.
const List<String> fixturePackageDocuments = [
  flutterPackageJson,
  pythonPackageJson,
  gitPackageJson,
  vscodePackageJson,
  dockerPackageJson,
  nodejsPackageJson,
  goPackageJson,
  rustPackageJson,
  jdkPackageJson,
  kotlinPackageJson,
  dotnetPackageJson,
  androidToolsPackageJson,
  springBootPackageJson,
  nextjsPackageJson,
  androidStudioPackageJson,
  intellijPackageJson,
  gradlePackageJson,
  mavenPackageJson,
  cmakePackageJson,
  postgresqlPackageJson,
  redisPackageJson,
  sqlitePackageJson,
  ollamaPackageJson,
];

const List<String> fixtureProfileDocuments = [
  flutterProfileJson,
  pythonProfileJson,
];
