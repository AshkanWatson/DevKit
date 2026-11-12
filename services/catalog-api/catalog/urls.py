"""Catalog API routes under /api/v1/ (docs/api/catalog-api.md)."""
from django.urls import include, path
from rest_framework.routers import DefaultRouter

from . import views

# Contract paths have no trailing slash (docs/api/catalog-api.md).
router = DefaultRouter(trailing_slash=False)
router.register("catalog/packages", views.PackageViewSet, basename="packages")
router.register("catalog/profiles", views.ProfileViewSet, basename="profiles")

urlpatterns = [
    path("health", views.health, name="health"),
    path("catalog/categories", views.categories, name="categories"),
    path(
        "catalog/packages/<str:package_id>/versions",
        views.package_versions,
        name="package-versions",
    ),
    path(
        "catalog/packages/<str:package_id>/manifest",
        views.package_manifest_download,
        name="package-manifest",
    ),
    path("", include(router.urls)),
]
