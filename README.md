# DevKit

**DevKit** is a cross-platform development environment manager. It helps developers discover, install, manage, and switch between programming languages, frameworks, IDEs, SDKs, and development tools.

> **Project status:** Phase 0 architecture and handoff. This repository is a specification/starter structure, not a functioning installer yet.

## Product direction

DevKit has two primary installation experiences:

- **Individual packages:** choose a tool and a specific version, such as Flutter 3.x, Python 3.12, or Rust stable.
- **Developer Profiles:** start from an editable collection of packages, such as Flutter Developer or Python Developer.

The core product requirement is reliable version management: multiple versions can coexist, users can select an active version, and future releases can add project-specific version pinning.

## Architecture at a glance

- **Desktop UI:** Flutter
- **Local engine:** Rust
- **Flutter ↔ Rust:** FFI using `flutter_rust_bridge`
- **Remote catalog:** Django REST Framework
- **Remote database:** PostgreSQL
- **Local state and cache:** SQLite
- **Package/profile definitions:** versioned JSON manifests

Read [HANDOFF.md](HANDOFF.md) for the implementation contract and [DESIGN.md](DESIGN.md) for the UI/UX specification.

## Repository map

```text
devkit/
├── apps/desktop/              # Flutter desktop application
├── crates/                   # Rust engine and FFI bridge
├── services/catalog-api/      # Django REST Framework catalog API
├── schemas/                   # JSON Schema contracts
├── docs/                      # Architecture and product documentation
├── packaging/                 # Platform packaging configuration
├── scripts/                   # Build and developer scripts
├── .github/workflows/         # CI and release automation
├── HANDOFF.md
├── DESIGN.md
├── CONTRIBUTING.md
├── SECURITY.md
├── LICENSE
└── README.md
```

## Development status and planned milestones

1. **Phase 0 — Architecture:** contracts, security model, schemas, UI specification, and delivery plan.
2. **Phase 1 — Desktop foundation:** Flutter shell, Rust bridge, local fixture catalog, navigation, and simulated installation plans.
3. **Phase 2 — Catalog:** Django API, PostgreSQL, manifest publishing, local cache, and offline catalog browsing.
4. **Phase 3 — Managed installation:** download, verify, install, register, activate, rollback, and recovery.
5. **Phase 4 — Profiles and project environments:** editable profiles, project pinning, and environment diagnostics.
6. **Phase 5 — Production hardening:** signing, release automation, accessibility, telemetry opt-in, and platform-specific QA.

## License and project identity

This project is distributed under a **source-available, non-commercial license draft**, not an OSI-approved Open Source license. Commercial use/resale is restricted and attribution/copyright notices must be retained. See [LICENSE](LICENSE).

If the goal changes to a genuinely Open Source project, replace this license with an OSI-approved license (for example, Apache-2.0 or MIT) and accept that those licenses generally cannot prohibit commercial redistribution.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Contributions should include tests and documentation updates where applicable.

## Security

Do not submit credentials, signing keys, tokens, or private package source URLs. Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
