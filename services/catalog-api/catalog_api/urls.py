"""Catalog API URL configuration. Root: /api/v1/ (docs/api/catalog-api.md)."""
from django.urls import include, path

urlpatterns = [
    path("api/v1/", include("catalog.urls")),
]
