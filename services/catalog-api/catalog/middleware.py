"""Pagination, error envelope, and ETag middleware (docs/api/catalog-api.md)."""
from __future__ import annotations

import hashlib
import uuid

from django.core.exceptions import ValidationError
from django.http import Http404, JsonResponse
from rest_framework.exceptions import APIException
from rest_framework.pagination import PageNumberPagination


class StandardPagination(PageNumberPagination):
    """Stable envelope: count / next / previous / results (contract v1)."""

    page_size = 30
    page_size_query_param = "page_size"
    max_page_size = 100


def error_envelope(code: str, message: str, status: int, request=None):
    request_id = "unavailable"
    if request is not None:
        request_id = getattr(request, "_devkit_request_id", None) or request.headers.get(
            "X-Request-Id"
        ) or uuid.uuid4().hex[:12]
    payload = {
        "error": {"code": code, "message": message, "request_id": request_id}
    }
    response = JsonResponse(payload, status=status, json_dumps_params={"sort_keys": True})
    response["X-Request-Id"] = request_id
    return response


def error_envelope_handler(exc, context):
    """DRF exception handler producing the documented error envelope."""
    request = context.get("request")
    if isinstance(exc, Http404):
        return error_envelope("NOT_FOUND", "The requested resource was not found.", 404, request)
    if isinstance(exc, APIException):
        return error_envelope(
            getattr(exc, "default_code", "INTERNAL_ERROR").upper(),
            str(exc.detail) if getattr(exc, "detail", None) else exc.default_detail,
            exc.status_code,
            request,
        )
    if isinstance(exc, ValidationError):
        return error_envelope("VALIDATION_FAILED", "; ".join(exc.messages), 400, request)
    return error_envelope("INTERNAL_ERROR", "An internal error occurred.", 500, request)


class ETagCacheMiddleware:
    """ETag + Cache-Control for JSON responses (docs/api/catalog-api.md).

    Applies to successful GET responses under /api/v1/: strong ETag from the
    body, If-None-Match honored with 304.
    """

    CACHE_CONTROL = "public, max-age=300"

    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        if request.method == "GET":
            if_none_match = request.headers.get("If-None-Match")

        response = self.get_response(request)

        if request.method != "GET":
            return response
        if request.path.startswith("/api/v1/") and response.status_code == 200:
            body = response.content
            etag = '"%s"' % hashlib.sha256(body).hexdigest()[:32]
            response["ETag"] = etag
            response["Cache-Control"] = self.CACHE_CONTROL
            if if_none_match and if_none_match.strip() == etag:
                not_modified = JsonResponse({}, status=304)
                not_modified["ETag"] = etag
                not_modified["Cache-Control"] = self.CACHE_CONTROL
                return not_modified
        return response
