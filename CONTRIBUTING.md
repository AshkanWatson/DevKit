# Contributing to DevKit

Thanks for considering a contribution.

## Before you start

1. Read `README.md`, `HANDOFF.md`, and `DESIGN.md`.
2. Check existing issues and open a discussion for large changes.
3. For architecture changes, propose an ADR before implementation.
4. Confirm that your contribution is compatible with the repository's source-available license.

## Contribution workflow

1. Fork the repository and create a focused branch.
2. Make a small, reviewable change.
3. Add or update tests.
4. Run the relevant formatters, analyzers, and tests.
5. Update documentation and schemas if behavior or contracts changed.
6. Open a pull request describing motivation, implementation, tests, and limitations.

## Engineering expectations

- Keep UI, domain logic, and platform operations separated.
- Do not add arbitrary shell execution from package metadata.
- Use fixture data in automated tests; never rely on live vendor downloads.
- Do not commit secrets, access tokens, signing certificates, or personal data.
- Avoid unrelated formatting changes.
- Preserve backward compatibility for published manifest and API contracts where possible.

## Pull request checklist

- [ ] The change has a clear scope.
- [ ] Tests were added or updated.
- [ ] Relevant tests pass locally.
- [ ] Documentation/schema updates are included.
- [ ] No secrets or private data are included.
- [ ] Security implications have been considered.
