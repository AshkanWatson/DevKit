"""Django admin: the publication surface (P2-02).

Publication endpoints are intentionally omitted from the public client
contract (docs/api/catalog-api.md). Maintainers publish/unpublish here,
behind Django's admin authentication, and every publish runs the
validation gate in catalog/publishing.py.
"""
from __future__ import annotations

from django.contrib import admin, messages

from . import publishing
from .models import (
    Artifact,
    Package,
    PackagePlatform,
    PackageVersion,
    Profile,
    ProfilePackage,
)


class ArtifactInline(admin.TabularInline):
    model = Artifact
    extra = 0


class PackageVersionInline(admin.StackedInline):
    model = PackageVersion
    extra = 0
    show_change_link = True


class PackagePlatformInline(admin.TabularInline):
    model = PackagePlatform
    extra = 0


@admin.register(Package)
class PackageAdmin(admin.ModelAdmin):
    list_display = ("id", "name", "category", "is_published", "updated_at")
    list_filter = ("category", "is_published")
    search_fields = ("id", "name", "publisher")
    inlines = [PackagePlatformInline]
    actions = ["publish_selected", "unpublish_selected"]

    @admin.action(description="Validate and publish selected packages")
    def publish_selected(self, request, queryset):
        published = 0
        for package in queryset:
            try:
                publishing.publish_package(package)
                published += 1
            except Exception as exc:  # noqa: BLE001 — surface per-item failure
                self.message_user(
                    request,
                    f"{package.id}: not published ({exc})",
                    level=messages.ERROR,
                )
        if published:
            self.message_user(request, f"{published} package(s) published.")

    @admin.action(description="Unpublish selected packages")
    def unpublish_selected(self, request, queryset):
        for package in queryset:
            publishing.unpublish_package(package)
        self.message_user(request, f"{queryset.count()} package(s) unpublished.")


@admin.register(PackageVersion)
class PackageVersionAdmin(admin.ModelAdmin):
    list_display = ("package", "version", "channel", "release_date")
    list_filter = ("channel",)
    inlines = [ArtifactInline]


@admin.register(Profile)
class ProfileAdmin(admin.ModelAdmin):
    list_display = ("id", "name", "is_published", "updated_at")
    inlines = []
    actions = ["publish_selected", "unpublish_selected"]

    @admin.action(description="Validate and publish selected profiles")
    def publish_selected(self, request, queryset):
        for profile in queryset:
            try:
                publishing.publish_profile(profile)
            except Exception as exc:  # noqa: BLE001
                self.message_user(request, f"{profile.id}: {exc}", level=messages.ERROR)

    @admin.action(description="Unpublish selected profiles")
    def unpublish_selected(self, request, queryset):
        for profile in queryset:
            publishing.unpublish_profile(profile)


admin.site.register(ProfilePackage)
