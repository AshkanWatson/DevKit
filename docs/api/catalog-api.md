# Catalog API Contract — v1

> **Status:** Phase 2 core implemented in `services/catalog-api/` (models, publication gate, public reads, pagination, ETag, error envelope) and client-side cache in `crates/devkit-catalog/src/remote.rs` (P2-04: ETag revalidation, offline reads from the SQLite `catalog_cache`, HTTPS-only client). Not yet implemented: production deployment. Admin publication lives in Django admin, outside this contract.
>
> **Client behavior note (P2-04):** the DevKit client pins `/api/v1/`, sends `If-None-Match` on every request when it holds a validator, and treats a `304` as "cached payload is current" — unchanged catalogs are never re-downloaded. Network unavailability degrades to cache-only browsing (the client never sends credentials, and a malformed payload is treated as a contract violation, not an offline condition). The client ignores unknown fields defensively; the list pages it follows use the `next` links as issued by the server.

Base path: `/api/v1/`

## Public read endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/catalog/packages` | Search/filter/paginate published packages |
| GET | `/catalog/packages/{package_id}` | Package metadata (full manifest) |
| GET | `/catalog/packages/{package_id}/versions` | Published versions and platform artifacts |
| GET | `/catalog/packages/{package_id}/manifest` | Raw JSON manifest document (download) |
| GET | `/catalog/profiles` | Published profiles |
| GET | `/catalog/profiles/{profile_id}` | Profile detail |
| GET | `/catalog/categories` | Available categories with published counts |
| GET | `/health` | Service health |

## Query conventions

`GET /catalog/packages?q=python&category=language&platform=windows-x64&page=1&page_size=30`

- `q`: substring match on name/description.
- `category`: one of the schema categories (`language`, `framework`, `sdk`, `ide`, `build_tool`, `database`, `utility`, `ai_tool`, `other`).
- `platform`: `os-arch` pair from the manifest platform enum (e.g. `windows-x64`, `macos-arm64`).
- `page`, `page_size`: pagination; `page_size` defaults to 30, maximum 100.

### Pagination shape

Stable envelope on every list endpoint:

```json
{
  "count": 128,
  "next": "https://catalog.example/api/v1/catalog/packages?page=2",
  "previous": null,
  "results": []
}
```

- `next`/`previous` are absolute URLs or `null`.
- Results are ordered deterministically (by package ID) so clients can cache and diff.

### Caching and conditional requests

- List and detail responses send `ETag` and honor `If-None-Match` with `304` (middleware on all `/api/v1/` GET 200s).
- `Cache-Control: public, max-age=300` is set on every successful GET; the client stores both payload and validator in `catalog_cache` (docs/architecture/local-database.md).

### Identity and timestamps

- Stable package IDs; display names may change.
- Every manifest-bearing response includes `schema_version`.
- Timestamps are UTC ISO-8601.

## Response bodies

`GET /catalog/packages/{package_id}` returns the package manifest as published (`schemas/package.schema.json`), i.e. the full record including `versions` with artifact descriptors. The list endpoint returns summaries only (identity, display fields, category, platform support, default policy) — not artifact lists — so list caching stays cheap.

`GET /catalog/packages/{package_id}/versions` returns `{"count": n, "results": [version objects]}` ordered newest first (release date, then version).

`GET /catalog/categories` returns `{"results": [{"id": "language", "package_count": 3}, ...]}` covering all schema categories (unpaginated; the list is small and stable).

`GET /catalog/profiles/{profile_id}` returns the profile manifest (`schemas/profile.schema.json`).

## Error response

All non-2xx responses use this envelope:

```json
{
  "error": {
    "code": "PACKAGE_NOT_FOUND",
    "message": "The requested package was not found.",
    "request_id": "opaque-request-id"
  }
}
```

Codes: `NOT_FOUND` (unknown resource), `PACKAGE_NOT_FOUND`, `PROFILE_NOT_FOUND`, `INVALID_ARGUMENT`, `RATE_LIMITED`, `VALIDATION_FAILED`, `INTERNAL_ERROR`. Unmatched routes return `NOT_FOUND`.

Never expose stack traces, secrets, internal storage paths, or unpublished package metadata.

## Authentication and authorization

- **Public read endpoints require no authentication.** Catalog browsing must work without accounts, and the client must never send credentials to these endpoints.
- Unauthenticated clients are subject to rate limiting (`RATE_LIMITED` above) to protect the service.
- **Publication/admin endpoints are intentionally omitted from the public client contract.** They require authenticated maintainer roles, audit logs, schema validation, source validation, and explicit publish/unpublish actions (defined in Phase 2).
- The API never proxies artifact downloads; clients download from the manifest-declared official source directly (docs/architecture/catalog-flow.md).

## Real catalog data and URL verification

The catalog serves **real vendor data**: version-pinned official release URLs with the exact SHA-256 and byte size measured from the actual published files. Placeholder/example hosts (`example.invalid`) exist only in the test suite.

- **Seeding (maintainer, offline):** `python manage.py seed_real_catalog` upserts the real packages (`devkit.lang.python` 3.12.8 embeddable, `devkit.utility.ripgrep` 14.1.1, `devkit.utility.fd` 10.2.0) and publishes them through the normal validation gate. It writes metadata only — it never downloads. The recorded facts were measured from the vendor releases on 2026-11-15; if a vendor replaces a file, re-measure and update `catalog/management/commands/seed_real_catalog.py`.
- **Verification (maintainer, online):** `python scripts/verify_catalog_artifacts.py <base-url>` walks the published catalog through the public API and re-downloads every artifact, asserting HTTPS-only, the recorded `size_bytes`, and the recorded SHA-256. Exit 0 = every artifact is real. Run it after any seed-data change or vendor release refresh.
- **Pipeline proof (maintainer, online):** `DEVKIT_REAL_ARTIFACT=1 cargo run -p devkit-jobs --example real_install` downloads the real Python 3.12.8 embeddable release through the actual install pipeline (real byte-count progress, SHA-256 gate, extraction, receipt, validation, uninstall). Note: sources that redirect to a different host (GitHub release URLs → CDN) are declined by the same-host redirect policy (security-model.md §3.3) — direct-serving sources like python.org are used for the proof; see tasks/BACKLOG.md.
- **Local dev server:** `DEVKIT_DEBUG=true DEVKIT_ALLOWED_HOSTS=127.0.0.1 python manage.py runserver 127.0.0.1:8000`. The base URL may be plain HTTP for the local loopback carve-out (security-model.md §3.1); artifact URLs themselves must always be HTTPS.
- Tests never touch the network (testing-strategy.md): the seed command has offline tests (`catalog/tests.py::SeedRealCatalogTests`); real-URL verification is the script's job, run by a maintainer.

## API versioning

- Versioned from day one under `/api/v1/`.
- Additive changes (new optional fields, new query parameters, new endpoints) may land within v1 without client breakage.
- Breaking changes require a new version prefix and a documented deprecation window; the client pins the version it speaks and ignores unknown fields defensively.
- Each manifest-bearing response carries its own `schema_version` (independent of the URL version).
