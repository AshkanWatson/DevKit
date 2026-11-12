"""Phase 0 tests: health endpoint and contract placeholders."""
import json

from django.test import TestCase


class HealthEndpointTests(TestCase):
    def test_health_returns_ok_and_api_version(self):
        response = self.client.get("/api/v1/health")
        self.assertEqual(response.status_code, 200)
        payload = json.loads(response.content)
        self.assertEqual(payload["status"], "ok")
        self.assertEqual(payload["api_version"], "v1")

    def test_health_is_json(self):
        response = self.client.get("/api/v1/health")
        self.assertEqual(response["Content-Type"], "application/json")


class ContractPlaceholderTests(TestCase):
    """Catalog endpoints exist but must not pretend to serve data yet."""

    def test_packages_endpoint_returns_documented_error_envelope(self):
        response = self.client.get("/api/v1/catalog/packages")
        self.assertEqual(response.status_code, 501)
        payload = json.loads(response.content)
        self.assertEqual(payload["error"]["code"], "NOT_IMPLEMENTED")
        self.assertIn("request_id", payload["error"])

    def test_package_detail_returns_error_envelope(self):
        response = self.client.get("/api/v1/catalog/packages/devkit.lang.python")
        self.assertEqual(response.status_code, 501)
        payload = json.loads(response.content)
        self.assertEqual(payload["error"]["code"], "NOT_IMPLEMENTED")
