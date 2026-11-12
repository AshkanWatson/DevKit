"""Public read endpoints (P2-03, docs/api/catalog-api.md).

Only published records are exposed. No authentication on reads. The
publication/admin surface stays in Django admin (catalog/admin.py) with
role checks, outside the public client contract.
"""
from __future__ import annotations

import json

from django.db.models import Q
from django.http import JsonResponse
from django.utils.dateparse import parse_date
from rest_framework import viewsets
from rest_framework.decorators import api_view, authentication_classes, permission_classes

from .manifests import package_to_manifest, package_to_summary, profile_to_manifest
from .middleware import StandardPagination, error_envelope
from .models import Package, PackageVersion, Profile


def _no_auth(view):
    """Explicit: public reads use no authentication and no permissions."""
    view.authentication_classes = []
    view.permission_classes = []
    return view


class PackageViewSet(viewsets.ReadOnlyModelViewSet):
    """List summaries and detail manifests for published packages."""

    pagination_class = StandardPagination
    lookup_field = "id"
    # Package IDs are dotted (devkit.lang.python); the default DRF lookup
    # regex excludes dots.
    lookup_value_regex = "[^/]+"

    def get_queryset(self):
        queryset = Package.objects.filter(is_published=True).prefetch_related(
            "platforms", "versions", "versions__artifacts"
        )
        params = self.request.query_params
        query = params.get("q")
        if query:
            queryset = queryset.filter(Q(name__icontains=query) | Q(description__icontains=query))
        category = params.get("category")
        if category:
            queryset = queryset.filter(category=category)
        platform = params.get("platform")
        if platform:
            queryset = queryset.filter(platforms__platform=platform).distinct()
        return queryset

    def list(self, request, *args, **kwargs):
        queryset = self.get_queryset()
        paginator = self.pagination_class()
        page = paginator.paginate_queryset(queryset, request)
        return paginator.get_paginated_response([package_to_summary(p) for p in page])

    def retrieve(self, request, *args, **kwargs):
        package = self.get_object()
        return JsonResponse(package_to_manifest(package), json_dumps_params={"sort_keys": True})


def package_versions(request, package_id: str):
    """Published versions with artifacts for one package, newest first."""
    try:
        package = Package.objects.get(id=package_id, is_published=True)
    except Package.DoesNotExist:
        return error_envelope(
            "PACKAGE_NOT_FOUND", "The requested package was not found.", 404, request
        )
    versions = package.versions.all().prefetch_related("artifacts")
    versions = sorted(
        versions,
        key=lambda v: (
            v.release_date is None,
            v.release_date or parse_date("0001-01-01"),
            v.version,
        ),
        reverse=True,
    )
    from .manifests import version_to_manifest

    return JsonResponse(
        {"count": len(versions), "results": [version_to_manifest(v) for v in versions]},
        json_dumps_params={"sort_keys": True},
    )


def package_manifest_download(request, package_id: str):
    """Raw JSON manifest document (convenience for caching clients)."""
    try:
        package = Package.objects.get(id=package_id, is_published=True)
    except Package.DoesNotExist:
        return error_envelope(
            "PACKAGE_NOT_FOUND", "The requested package was not found.", 404, request
        )
    response = JsonResponse(package_to_manifest(package), json_dumps_params={"sort_keys": True})
    response["Content-Disposition"] = f'attachment; filename="{package_id}.json"'
    return response


class ProfileViewSet(viewsets.ReadOnlyModelViewSet):
    pagination_class = StandardPagination
    lookup_field = "id"
    lookup_value_regex = "[^/]+"

    def get_queryset(self):
        return Profile.objects.filter(is_published=True).prefetch_related(
            "platforms", "packages"
        )

    def list(self, request, *args, **kwargs):
        paginator = self.pagination_class()
        page = paginator.paginate_queryset(self.get_queryset(), request)
        return paginator.get_paginated_response([profile_to_manifest(p) for p in page])

    def retrieve(self, request, *args, **kwargs):
        return JsonResponse(
            profile_to_manifest(self.get_object()), json_dumps_params={"sort_keys": True}
        )


def categories(request):
    """Available categories with published package counts."""
    counts = (
        Package.objects.filter(is_published=True)
        .values("category")
        .order_by("category")
    )
    totals: dict[str, int] = {}
    for row in counts:
        totals[row["category"]] = totals.get(row["category"], 0) + 1
    all_categories = [c for c, _ in Package.Category.choices]
    results = [
        {"id": category, "package_count": totals.get(category, 0)}
        for category in all_categories
    ]
    return JsonResponse({"results": results}, json_dumps_params={"sort_keys": True})


@api_view(["GET"])
@authentication_classes([])
@permission_classes([])
def health(request):
    return JsonResponse(
        {"status": "ok", "api_version": "v1"}, json_dumps_params={"sort_keys": True}
    )
