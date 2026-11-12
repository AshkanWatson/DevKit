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
├── apps/desktop/               # Flutter desktop application (scaffolded)
├── crates/                     # Rust engine workspace (devkit-core scaffolded)
├── services/catalog-api/       # Django REST Framework catalog API (scaffolded)
├── schemas/                    # JSON Schema contracts + examples
├── docs/                       # Architecture, API, FFI, security, testing, phases
│   └── architecture/decisions/ # ADRs
├── tasks/                      # TODO.md and BACKLOG.md
├── scripts/                    # Developer and validation scripts
├── .github/workflows/          # CI (ci.yml) and release (release.yml)
├── PLAN.md                     # Roadmap and phase sequencing
├── HANDOFF.md                  # Implementation contract
├── DESIGN.md                   # UI/UX specification
├── CONTRIBUTING.md
├── SECURITY.md
├── LICENSE
└── README.md
```

Platform packaging configuration arrives in Phase 5 (see `docs/phases/`).

## Getting started

Prerequisites: Flutter (desktop), Rust (stable), Python 3.12+.

```powershell
# Flutter app
cd apps/desktop
flutter pub get
flutter analyze && flutter test

# Rust engine
cd crates
cargo test --workspace

# Catalog API service
cd services/catalog-api
python -m pip install -r requirements.txt
python manage.py check
python manage.py test

# Validate JSON schemas and examples (also runs in CI)
python scripts/validate_schemas.py
```

Formatting conventions are enforced by `.editorconfig`, `dart format`, `cargo fmt`, and the CI workflows.

## Development status and planned milestones

**Current status: Phase 0 complete** — contracts, schemas, security model, testing strategy, CI, and project scaffolds. See `docs/phases/PHASE_0.md` for the verification log and `tasks/TODO.md` for current tasks.

1. **Phase 0 — Architecture:** [x] contracts, security model, schemas, UI specification, and delivery plan.
2. **Phase 1 — Desktop foundation:** [ ] Flutter shell, Rust bridge, local fixture catalog, navigation, and simulated installation plans.
3. **Phase 2 — Catalog:** [ ] Django API, PostgreSQL, manifest publishing, local cache, and offline catalog browsing.
4. **Phase 3 — Managed installation:** [ ] download, verify, install, register, activate, rollback, and recovery.
5. **Phase 4 — Profiles and project environments:** [ ] editable profiles, project pinning, and environment diagnostics.
6. **Phase 5 — Production hardening:** [ ] signing, release automation, accessibility, telemetry opt-in, and platform-specific QA.

## License and project identity

This project is distributed under a **source-available, non-commercial license draft**, not an OSI-approved Open Source license. Commercial use/resale is restricted and attribution/copyright notices must be retained. See [LICENSE](LICENSE).

If the goal changes to a genuinely Open Source project, replace this license with an OSI-approved license (for example, Apache-2.0 or MIT) and accept that those licenses generally cannot prohibit commercial redistribution.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Contributions should include tests and documentation updates where applicable.

## Security

Do not submit credentials, signing keys, tokens, or private package source URLs. Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
