# Catalog API Contract — v1

> **Status:** Phase 2 core implemented in `services/catalog-api/` (models, publication gate, public reads, pagination, ETag, error envelope). Not yet implemented: client-side cache (P2-04, waits on the Phase 1 bridge) and production deployment. Admin publication lives in Django admin, outside this contract.

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

## API versioning

- Versioned from day one under `/api/v1/`.
- Additive changes (new optional fields, new query parameters, new endpoints) may land within v1 without client breakage.
- Breaking changes require a new version prefix and a documented deprecation window; the client pins the version it speaks and ignores unknown fields defensively.
- Each manifest-bearing response carries its own `schema_version` (independent of the URL version).
