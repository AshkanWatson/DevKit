"""Catalog API routes under /api/v1/ (docs/api/catalog-api.md).

Endpoints marked "Phase 2" are part of the published contract; they respond
with the documented error envelope (501 NOT_IMPLEMENTED) until implemented.
"""
from django.urls import path

from . import views

urlpatterns = [
    path("health", views.health, name="health"),
    # Phase 2: catalog read endpoints.
    path("catalog/packages", views.not_implemented, name="packages"),
    path("catalog/packages/<str:package_id>", views.not_implemented, name="package-detail"),
    path("catalog/packages/<str:package_id>/versions", views.not_implemented, name="package-versions"),
    path("catalog/profiles", views.not_implemented, name="profiles"),
    path("catalog/profiles/<str:profile_id>", views.not_implemented, name="profile-detail"),
    path("catalog/categories", views.not_implemented, name="categories"),
]
