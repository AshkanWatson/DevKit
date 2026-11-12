"""Catalog data model (P2-01, P2-02).

Mirrors schemas/package.schema.json and schemas/profile.schema.json.
Publication state gates every public read endpoint: records are invisible
until explicitly published, and publish/unpublish runs validation
(catalog/publishing.py).
"""
from __future__ import annotations

from django.core.exceptions import ValidationError
from django.db import models


class Package(models.Model):
    """A named product with versioned releases and platform artifacts."""

    class Category(models.TextChoices):
        LANGUAGE = "language"
        FRAMEWORK = "framework"
        SDK = "sdk"
        IDE = "ide"
        BUILD_TOOL = "build_tool"
        DATABASE = "database"
        UTILITY = "utility"
        AI_TOOL = "ai_tool"
        OTHER = "other"

    class VersionPolicy(models.TextChoices):
        EXACT = "exact"
        STABLE = "stable"
        LATEST = "latest"
        CHANNEL = "channel"

    SCHEMA_VERSION = 1

    id = models.SlugField(primary_key=True, max_length=100)  # devkit.lang.python
    name = models.CharField(max_length=200)
    description = models.TextField(blank=True)
    publisher = models.CharField(max_length=200)
    homepage = models.URLField()
    category = models.CharField(max_length=20, choices=Category.choices)
    default_version_policy = models.CharField(
        max_length=10, choices=VersionPolicy.choices, default=VersionPolicy.STABLE
    )
    license_expression = models.CharField(max_length=200, blank=True)
    maintainer = models.CharField(max_length=200, blank=True)
    schema_version = models.PositiveIntegerField(default=SCHEMA_VERSION)

    # Publication workflow: draft records are invisible to public endpoints.
    is_published = models.BooleanField(default=False)
    published_at = models.DateTimeField(null=True, blank=True)

    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["id"]
        verbose_name_plural = "packages"

    def __str__(self) -> str:  # pragma: no cover - trivial
        return self.id


class PackagePlatform(models.Model):
    """Platform/architecture support row (os-arch), DB-agnostic filtering."""

    package = models.ForeignKey(
        Package, on_delete=models.CASCADE, related_name="platforms"
    )
    platform = models.CharField(max_length=20)  # windows-x64, macos-arm64, ...

    class Meta:
        constraints = [
            models.UniqueConstraint(
                fields=["package", "platform"], name="unique_package_platform"
            )
        ]
        ordering = ["platform"]

    def clean(self) -> None:
        valid = {
            f"{os_}-{arch}"
            for os_ in ("windows", "macos", "linux")
            for arch in ("x64", "arm64")
        }
        if self.platform not in valid:
            raise ValidationError(f"Unsupported platform: {self.platform}")


class PackageVersion(models.Model):
    """A published release of a package."""

    class Channel(models.TextChoices):
        STABLE = "stable"
        BETA = "beta"
        DEV = "dev"
        NIGHTLY = "nightly"

    package = models.ForeignKey(
        Package, on_delete=models.CASCADE, related_name="versions"
    )
    version = models.CharField(max_length=100)
    channel = models.CharField(max_length=10, choices=Channel.choices)
    release_date = models.DateField(null=True, blank=True)
    release_notes_url = models.URLField(blank=True)
    # Dependency records per schemas/package.schema.json (package_id,
    # required, version_policy, optional version/channel).
    dependencies = models.JSONField(default=list, blank=True)

    class Meta:
        constraints = [
            models.UniqueConstraint(
                fields=["package", "version"], name="unique_package_version"
            )
        ]
        ordering = ["package", "version"]

    @property
    def is_prerelease(self) -> bool:
        return self.channel != self.Channel.STABLE

    def __str__(self) -> str:  # pragma: no cover - trivial
        return f"{self.package_id} {self.version}"


class Artifact(models.Model):
    """A platform-specific downloadable artifact of a version."""

    class OS(models.TextChoices):
        WINDOWS = "windows"
        MACOS = "macos"
        LINUX = "linux"

    class Architecture(models.TextChoices):
        X64 = "x64"
        ARM64 = "arm64"

    class Installer(models.TextChoices):
        ARCHIVE = "archive"
        EXE = "exe"
        MSI = "msi"
        DMG = "dmg"
        PKG = "pkg"
        APPIMAGE = "appimage"
        EXTERNAL_PACKAGE_MANAGER = "external_package_manager"

    package_version = models.ForeignKey(
        PackageVersion, on_delete=models.CASCADE, related_name="artifacts"
    )
    os = models.CharField(max_length=10, choices=OS.choices)
    architecture = models.CharField(max_length=6, choices=Architecture.choices)
    installer = models.CharField(max_length=32, choices=Installer.choices)
    url = models.URLField()
    sha256 = models.CharField(max_length=64)
    size_bytes = models.BigIntegerField()
    signature_url = models.URLField(blank=True)
    publisher_identity = models.CharField(max_length=200, blank=True)
    archive_root = models.CharField(max_length=200, blank=True)
    executable_relative_path = models.CharField(max_length=200, blank=True)

    class Meta:
        constraints = [
            models.UniqueConstraint(
                fields=["package_version", "os", "architecture", "installer"],
                name="unique_artifact_target",
            )
        ]
        ordering = ["os", "architecture"]

    def __str__(self) -> str:  # pragma: no cover - trivial
        return f"{self.package_version} {self.os}/{self.architecture}"


class Profile(models.Model):
    """A Developer Profile: an editable list of package requirements."""

    SCHEMA_VERSION = 1

    id = models.SlugField(primary_key=True, max_length=100)
    name = models.CharField(max_length=200)
    description = models.TextField(blank=True)
    maintainer = models.CharField(max_length=200, blank=True)
    schema_version = models.PositiveIntegerField(default=SCHEMA_VERSION)

    is_published = models.BooleanField(default=False)
    published_at = models.DateTimeField(null=True, blank=True)

    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["id"]

    def __str__(self) -> str:  # pragma: no cover - trivial
        return self.id


class ProfilePlatform(models.Model):
    profile = models.ForeignKey(
        Profile, on_delete=models.CASCADE, related_name="platforms"
    )
    platform = models.CharField(max_length=20)

    class Meta:
        constraints = [
            models.UniqueConstraint(
                fields=["profile", "platform"], name="unique_profile_platform"
            )
        ]


class ProfilePackage(models.Model):
    """Package membership inside a profile."""

    profile = models.ForeignKey(
        Profile, on_delete=models.CASCADE, related_name="packages"
    )
    package_id = models.SlugField(max_length=100)
    required = models.BooleanField(default=True)
    version_policy = models.CharField(max_length=10)  # validated in publishing
    version = models.CharField(max_length=100, blank=True)
    channel = models.CharField(max_length=20, blank=True)
    reason = models.TextField(blank=True)
    position = models.PositiveIntegerField(default=0)

    class Meta:
        constraints = [
            models.UniqueConstraint(
                fields=["profile", "package_id"], name="unique_profile_package"
            )
        ]
        ordering = ["position", "package_id"]
