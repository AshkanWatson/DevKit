"""Serialization to the manifest shapes defined by schemas/*.schema.json.

The detail endpoints return exactly the published manifest shape so the
client can validate and cache them as documents, not as ORM dumps.
"""
from __future__ import annotations

from .models import Artifact, Package, PackageVersion, Profile


def artifact_to_manifest(artifact: Artifact) -> dict:
    data = {
        "os": artifact.os,
        "architecture": artifact.architecture,
        "installer": artifact.installer,
        "url": artifact.url,
        "sha256": artifact.sha256,
        "size_bytes": artifact.size_bytes,
    }
    if artifact.signature_url:
        data["signature_url"] = artifact.signature_url
    if artifact.publisher_identity:
        data["publisher_identity"] = artifact.publisher_identity
    if artifact.archive_root:
        data["archive_root"] = artifact.archive_root
    if artifact.executable_relative_path:
        data["executable_relative_path"] = artifact.executable_relative_path
    return data


def version_to_manifest(version: PackageVersion) -> dict:
    data = {
        "version": version.version,
        "channel": version.channel,
        "artifacts": [artifact_to_manifest(a) for a in version.artifacts.all()],
    }
    if version.release_date:
        data["release_date"] = version.release_date.isoformat()
    if version.release_notes_url:
        data["release_notes_url"] = version.release_notes_url
    if version.dependencies:
        data["dependencies"] = version.dependencies
    return data


def package_to_manifest(package: Package) -> dict:
    """Full package manifest (schemas/package.schema.json shape)."""
    data = {
        "schema_version": package.schema_version,
        "id": package.id,
        "name": package.name,
        "description": package.description,
        "publisher": package.publisher,
        "homepage": package.homepage,
        "category": package.category,
        "default_version_policy": package.default_version_policy,
        "versions": [
            version_to_manifest(v) for v in package.versions.all()
        ],
        "license_expression": package.license_expression,
        "maintainer": package.maintainer,
        "updated_at": package.updated_at.isoformat().replace("+00:00", "Z"),
    }
    platforms = [p.platform for p in package.platforms.all()]
    if platforms:
        data["supported_platforms"] = platforms
    return data


def package_to_summary(package: Package) -> dict:
    """List-endpoint summary: no artifact lists (docs/api/catalog-api.md)."""
    data = {
        "id": package.id,
        "name": package.name,
        "description": package.description,
        "publisher": package.publisher,
        "category": package.category,
        "default_version_policy": package.default_version_policy,
        "updated_at": package.updated_at.isoformat().replace("+00:00", "Z"),
    }
    platforms = [p.platform for p in package.platforms.all()]
    if platforms:
        data["supported_platforms"] = platforms
    return data


def profile_to_manifest(profile: Profile) -> dict:
    data = {
        "schema_version": profile.schema_version,
        "id": profile.id,
        "name": profile.name,
        "description": profile.description,
        "packages": [
            {
                "package_id": entry.package_id,
                "required": entry.required,
                "version_policy": entry.version_policy,
                **({"version": entry.version} if entry.version else {}),
                **({"channel": entry.channel} if entry.channel else {}),
                **({"reason": entry.reason} if entry.reason else {}),
            }
            for entry in profile.packages.all()
        ],
    }
    if profile.maintainer:
        data["maintainer"] = profile.maintainer
    platforms = [p.platform for p in profile.platforms.all()]
    if platforms:
        data["supported_platforms"] = platforms
    return data
