# Security Policy

## Reporting a vulnerability

Please do not disclose security vulnerabilities in public issues.

Until a dedicated security contact is published, contact the project maintainers through the private security reporting feature of the hosting platform, if available. Include:
- affected version/commit
- impact and conditions
- reproduction steps or proof of concept
- suggested mitigation, if known

Do not include real credentials or user data in reports.

## Security-sensitive areas

DevKit handles third-party software installation. Treat the following as high-risk:
- catalog publication and manifest validation
- download URL and redirect validation
- artifact integrity/signature verification
- archive extraction and path handling
- installer process execution
- privilege elevation
- PATH/environment activation
- update and release signing

## Maintainer requirements

- Review package source provenance.
- Require hashes for published artifacts.
- Prefer publisher signatures where available.
- Never execute arbitrary commands from catalog metadata.
- Keep CI secrets out of logs.
- Use protected branches and reviewed release tags.
- Publish checksums for release artifacts.
- Rotate compromised credentials and revoke compromised signing identities promptly.
