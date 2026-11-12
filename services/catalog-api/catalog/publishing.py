"""Publication validation gate (P2-02).

A record may only be published after this validation passes: HTTPS-only
artifact URLs, well-formed SHA-256 checksums, at least one artifact per
version, and required metadata. Security policy: docs/security/security-model.md §2/§4.
"""
from __future__ import annotations

import re

from django.core.exceptions import ValidationError
from django.utils import timezone

from .models import Artifact, Package, PackageVersion, Profile

SHA256_PATTERN = re.compile(r"^[0-9a-fA-F]{64}$")
VALID_POLICIES = {"exact", "stable", "latest", "channel"}
VALID_PLATFORMS = {
    f"{os_}-{arch}"
    for os_ in ("windows", "macos", "linux")
    for arch in ("x64", "arm64")
}


def validate_package(package: Package) -> None:
    """Raise ValidationError unless `package` is fit for publication."""
    errors: list[str] = []

    if not package.license_expression:
        errors.append("license_expression is required for publication.")
    if not package.maintainer:
        errors.append("maintainer is required for publication.")
    for platform in package.platforms.all():
        if platform.platform not in VALID_PLATFORMS:
            errors.append(f"Unsupported platform: {platform.platform}")

    versions = list(package.versions.all())
    if not versions:
        errors.append("At least one version is required for publication.")

    for version in versions:
        _validate_version(version, errors)

    if errors:
        raise ValidationError(errors)


def _validate_version(version: PackageVersion, errors: list[str]) -> None:
    artifacts = list(version.artifacts.all())
    if not artifacts:
        errors.append(f"Version {version.version} has no artifacts.")
    for artifact in artifacts:
        _validate_artifact(artifact, errors)
    for dependency in version.dependencies or []:
        errors.extend(_validate_dependency(version, dependency))


def _validate_artifact(artifact: Artifact, errors: list[str]) -> None:
    label = f"Version {artifact.package_version.version}, artifact {artifact.pk}:"
    if not artifact.url.lower().startswith("https://"):
        errors.append(f"{label} artifact URL must use HTTPS.")
    if artifact.url.startswith("https://") and "@" in artifact.url.split("//", 1)[1].split("/", 1)[0]:
        errors.append(f"{label} artifact URL must not contain credentials.")
    if not SHA256_PATTERN.fullmatch(artifact.sha256 or ""):
        errors.append(f"{label} sha256 must be 64 hex characters.")
    if artifact.size_bytes is None or artifact.size_bytes <= 0:
        errors.append(f"{label} size_bytes must be positive.")


def _validate_dependency(version: PackageVersion, dependency: dict) -> list[str]:
    problems: list[str] = []
    package_id = dependency.get("package_id", "")
    if not package_id or "." not in package_id:
        problems.append(f"Version {version.version}: dependency package_id is invalid.")
    policy = dependency.get("version_policy")
    if policy not in VALID_POLICIES:
        problems.append(f"Version {version.version}: dependency version_policy '{policy}' is invalid.")
    if policy == "exact" and not dependency.get("version"):
        problems.append(f"Version {version.version}: exact dependency requires a version.")
    if policy == "channel" and not dependency.get("channel"):
        problems.append(f"Version {version.version}: channel dependency requires a channel.")
    return problems


def publish_package(package: Package) -> None:
    """Validate and publish. Rolls publication back on validation failure."""
    validate_package(package)
    package.is_published = True
    package.published_at = timezone.now()
    package.save(update_fields=["is_published", "published_at", "updated_at"])


def unpublish_package(package: Package) -> None:
    package.is_published = False
    package.published_at = None
    package.save(update_fields=["is_published", "published_at", "updated_at"])


def validate_profile(profile: Profile) -> None:
    errors: list[str] = []
    for platform in profile.platforms.all():
        if platform.platform not in VALID_PLATFORMS:
            errors.append(f"Unsupported platform: {platform.platform}")
    entries = list(profile.packages.all())
    if not entries:
        errors.append("At least one package selection is required for publication.")
    for entry in entries:
        if entry.version_policy not in VALID_POLICIES:
            errors.append(f"Profile package {entry.package_id}: invalid version_policy.")
        if entry.version_policy == "exact" and not entry.version:
            errors.append(f"Profile package {entry.package_id}: exact policy requires a version.")
        if entry.version_policy == "channel" and not entry.channel:
            errors.append(f"Profile package {entry.package_id}: channel policy requires a channel.")
    if errors:
        raise ValidationError(errors)


def publish_profile(profile: Profile) -> None:
    validate_profile(profile)
    profile.is_published = True
    profile.published_at = timezone.now()
    profile.save(update_fields=["is_published", "published_at", "updated_at"])


def unpublish_profile(profile: Profile) -> None:
    profile.is_published = False
    profile.published_at = None
    profile.save(update_fields=["is_published", "published_at", "updated_at"])
