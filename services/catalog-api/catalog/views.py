"""Catalog API views.

Phase 0: only /health is functional. Catalog read endpoints exist to make
the documented contract (docs/api/catalog-api.md) explicit and return the
standard error envelope with 501 until Phase 2 implements them. Never
return fake package data.
"""
from django.http import JsonResponse

API_VERSION = "v1"


def error_response(code: str, message: str, status: int, request_id: str | None = None):
    """Error envelope from docs/api/catalog-api.md."""
    payload = {
        "error": {
            "code": code,
            "message": message,
            "request_id": request_id or "unavailable",
        }
    }
    return JsonResponse(payload, status=status, json_dumps_params={"sort_keys": True})


def health(request):
    """Service liveness probe."""
    return JsonResponse(
        {"status": "ok", "api_version": API_VERSION},
        json_dumps_params={"sort_keys": True},
    )


def not_implemented(request, *args, **kwargs):
    """Placeholder for catalog endpoints not yet implemented (Phase 2)."""
    return error_response(
        "NOT_IMPLEMENTED",
        "This catalog endpoint is part of the v1 contract but is not implemented yet.",
        status=501,
    )
