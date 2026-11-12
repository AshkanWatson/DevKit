"""Phase 2 tests: public read API, publication workflow, error envelope."""
import json

from django.test import TestCase

from . import publishing
from .models import (
    Artifact,
    Package,
    PackagePlatform,
    PackageVersion,
    Profile,
    ProfilePackage,
)


def make_artifact(version, **overrides):
    fields = dict(
        os="windows",
        architecture="x64",
        installer="archive",
        url="https://example.invalid/tool-1.0.0.zip",
        sha256="a" * 64,
        size_bytes=10_000_000,
    )
    fields.update(overrides)
    return Artifact.objects.create(package_version=version, **fields)


def make_published_package(package_id="devkit.util.demo", platforms=("windows-x64",)):
    package = Package.objects.create(
        id=package_id,
        name="Demo Tool",
        description="A demo tool for testing.",
        publisher="Demo Publisher",
        homepage="https://example.invalid/",
        category="utility",
        license_expression="MIT",
        maintainer="DevKit Catalog Team",
    )
    for platform in platforms:
        PackagePlatform.objects.create(package=package, platform=platform)
    version = PackageVersion.objects.create(
        package=package, version="1.0.0", channel="stable", release_date="2026-01-15"
    )
    make_artifact(version)
    publishing.publish_package(package)
    return package


class HealthEndpointTests(TestCase):
    def test_health_returns_ok_and_api_version(self):
        response = self.client.get("/api/v1/health")
        self.assertEqual(response.status_code, 200)
        payload = json.loads(response.content)
        self.assertEqual(payload["status"], "ok")
        self.assertEqual(payload["api_version"], "v1")


class PackageEndpointTests(TestCase):
    def setUp(self):
        self.package = make_published_package()

    def test_list_returns_pagination_envelope(self):
        response = self.client.get("/api/v1/catalog/packages")
        self.assertEqual(response.status_code, 200)
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 1)
        self.assertIsNone(payload["next"])
        self.assertIsNone(payload["previous"])
        self.assertEqual(len(payload["results"]), 1)
        summary = payload["results"][0]
        self.assertNotIn("versions", summary)  # summaries carry no artifacts
        self.assertEqual(summary["id"], self.package.id)

    def test_list_excludes_unpublished(self):
        Package.objects.create(
            id="devkit.util.draft",
            name="Draft",
            publisher="P",
            homepage="https://example.invalid/",
            category="utility",
        )
        payload = json.loads(self.client.get("/api/v1/catalog/packages").content)
        ids = [r["id"] for r in payload["results"]]
        self.assertNotIn("devkit.util.draft", ids)
        self.assertIn(self.package.id, ids)

    def test_detail_returns_manifest_shape(self):
        response = self.client.get(f"/api/v1/catalog/packages/{self.package.id}")
        self.assertEqual(response.status_code, 200)
        manifest = json.loads(response.content)
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["id"], self.package.id)
        self.assertEqual(manifest["default_version_policy"], "stable")
        self.assertEqual(manifest["license_expression"], "MIT")
        self.assertEqual(len(manifest["versions"]), 1)
        artifact = manifest["versions"][0]["artifacts"][0]
        self.assertEqual(artifact["os"], "windows")
        self.assertTrue(artifact["url"].startswith("https://"))

    def test_detail_404_uses_error_envelope(self):
        response = self.client.get("/api/v1/catalog/packages/devkit.lang.nope")
        self.assertEqual(response.status_code, 404)
        payload = json.loads(response.content)
        self.assertEqual(payload["error"]["code"], "NOT_FOUND")
        self.assertIn("request_id", payload["error"])
        self.assertTrue(response["X-Request-Id"])

    def test_filter_by_query_and_category(self):
        response = self.client.get("/api/v1/catalog/packages?q=demo&category=utility")
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 1)
        response = self.client.get("/api/v1/catalog/packages?q=zzz")
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 0)

    def test_filter_by_platform(self):
        PackagePlatform.objects.filter(package=self.package).delete()
        PackagePlatform.objects.create(package=self.package, platform="linux-x64")
        response = self.client.get("/api/v1/catalog/packages?platform=linux-x64")
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 1)
        response = self.client.get("/api/v1/catalog/packages?platform=windows-arm64")
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 0)

    def test_versions_endpoint(self):
        response = self.client.get(
            f"/api/v1/catalog/packages/{self.package.id}/versions"
        )
        self.assertEqual(response.status_code, 200)
        payload = json.loads(response.content)
        self.assertEqual(payload["count"], 1)
        self.assertEqual(payload["results"][0]["version"], "1.0.0")

    def test_etag_and_conditional_request(self):
        response = self.client.get("/api/v1/catalog/packages")
        self.assertIn("ETag", response)
        self.assertIn("Cache-Control", response)
        etag = response["ETag"]
        cached = self.client.get(
            "/api/v1/catalog/packages", headers={"If-None-Match": etag}
        )
        self.assertEqual(cached.status_code, 304)
        self.assertEqual(cached["ETag"], etag)

    def test_page_size_is_capped(self):
        for i in range(3):
            make_published_package(f"devkit.util.demo{i}", platforms=())
        response = self.client.get("/api/v1/catalog/packages?page_size=500")
        payload = json.loads(response.content)
        self.assertEqual(len(payload["results"]), 4)  # all pages collapse to max
        self.assertEqual(response.status_code, 200)


class ProfileEndpointTests(TestCase):
    def setUp(self):
        profile = Profile.objects.create(
            id="devkit.profile.demo",
            name="Demo Profile",
            description="Test profile.",
            maintainer="DevKit Catalog Team",
        )
        ProfilePackage.objects.create(
            profile=profile,
            package_id="devkit.lang.python",
            required=True,
            version_policy="exact",
            version="3.12.7",
            reason="Pinned runtime.",
        )
        publishing.publish_profile(profile)
        self.profile = profile

    def test_profiles_list_and_detail(self):
        payload = json.loads(self.client.get("/api/v1/catalog/profiles").content)
        self.assertEqual(payload["count"], 1)
        response = self.client.get("/api/v1/catalog/profiles/devkit.profile.demo")
        manifest = json.loads(response.content)
        self.assertEqual(manifest["packages"][0]["version_policy"], "exact")
        self.assertEqual(manifest["packages"][0]["version"], "3.12.7")

    def test_unpublished_profile_is_invisible(self):
        Profile.objects.create(
            id="devkit.profile.draft", name="Draft", description=""
        )
        payload = json.loads(self.client.get("/api/v1/catalog/profiles").content)
        ids = [r["id"] for r in payload["results"]]
        self.assertNotIn("devkit.profile.draft", ids)


class CategoriesEndpointTests(TestCase):
    def test_categories_include_published_counts(self):
        make_published_package()
        payload = json.loads(self.client.get("/api/v1/catalog/categories").content)
        by_id = {c["id"]: c["package_count"] for c in payload["results"]}
        self.assertEqual(by_id["utility"], 1)
        self.assertEqual(by_id["language"], 0)


class PublicationValidationTests(TestCase):
    def _draft(self):
        package = Package.objects.create(
            id="devkit.util.bad",
            name="Bad Package",
            publisher="P",
            homepage="https://example.invalid/",
            category="utility",
            license_expression="MIT",
            maintainer="M",
        )
        return package

    def test_cannot_publish_without_artifacts(self):
        package = self._draft()
        PackageVersion.objects.create(package=package, version="1.0.0", channel="stable")
        with self.assertRaises(Exception):
            publishing.publish_package(package)
        package.refresh_from_db()
        self.assertFalse(package.is_published)

    def test_cannot_publish_http_url(self):
        package = self._draft()
        version = PackageVersion.objects.create(
            package=package, version="1.0.0", channel="stable"
        )
        make_artifact(version, url="http://example.invalid/tool.zip")
        with self.assertRaises(Exception):
            publishing.publish_package(package)

    def test_cannot_publish_bad_sha256(self):
        package = self._draft()
        version = PackageVersion.objects.create(
            package=package, version="1.0.0", channel="stable"
        )
        make_artifact(version, sha256="deadbeef")
        with self.assertRaises(Exception):
            publishing.publish_package(package)

    def test_unpublish_hides_package(self):
        package = make_published_package()
        publishing.unpublish_package(package)
        payload = json.loads(self.client.get("/api/v1/catalog/packages").content)
        self.assertEqual(payload["count"], 0)

    def test_valid_package_publishes(self):
        package = self._draft()
        PackagePlatform.objects.create(package=package, platform="windows-x64")
        version = PackageVersion.objects.create(
            package=package, version="1.0.0", channel="stable"
        )
        make_artifact(version)
        publishing.publish_package(package)
        self.assertTrue(package.is_published)
        payload = json.loads(self.client.get("/api/v1/catalog/packages").content)
        self.assertEqual(payload["count"], 1)
