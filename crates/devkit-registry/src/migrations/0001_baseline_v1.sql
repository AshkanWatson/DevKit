-- Migration 0001: baseline schema v1.
-- Verbatim from docs/architecture/local-database.md §2 (authoritative).

CREATE TABLE schema_migrations (
    version     INTEGER PRIMARY KEY,          -- e.g. 1
    name        TEXT NOT NULL,
    applied_at  TEXT NOT NULL                 -- UTC ISO-8601
);

-- Cached package identity and metadata (from the remote catalog).
CREATE TABLE packages (
    id               TEXT PRIMARY KEY,        -- e.g. devkit.lang.python
    name             TEXT NOT NULL,
    description      TEXT NOT NULL,
    publisher        TEXT NOT NULL,
    homepage         TEXT NOT NULL,
    category         TEXT NOT NULL,
    default_version_policy TEXT NOT NULL CHECK (default_version_policy IN ('exact','stable','latest','channel')),
    manifest         TEXT NOT NULL,           -- full cached manifest JSON
    schema_version   INTEGER NOT NULL,
    updated_at       TEXT NOT NULL,
    cached_at        TEXT NOT NULL
);

-- Cached release metadata. One row per (package, version).
CREATE TABLE package_versions (
    package_id    TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    version       TEXT NOT NULL,
    channel       TEXT NOT NULL,
    release_date  TEXT,
    is_prerelease INTEGER NOT NULL DEFAULT 0,
    manifest_json TEXT NOT NULL,              -- version fragment as published
    cached_at     TEXT NOT NULL,
    PRIMARY KEY (package_id, version)
);

-- DevKit-managed installations. One row per installed concrete version.
CREATE TABLE installed_versions (
    package_id    TEXT NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    version       TEXT NOT NULL,
    install_path  TEXT NOT NULL,              -- managed-root-relative
    status        TEXT NOT NULL CHECK (status IN ('installing','installed','broken','removing')),
    receipt       TEXT NOT NULL,              -- InstallReceipt JSON
    installed_at  TEXT NOT NULL,
    PRIMARY KEY (package_id, version)
);

-- Global active version per package (version management doc §3).
CREATE TABLE active_versions (
    package_id    TEXT PRIMARY KEY REFERENCES packages(id) ON DELETE CASCADE,
    version       TEXT NOT NULL,
    activated_at  TEXT NOT NULL,
    FOREIGN KEY (package_id, version) REFERENCES installed_versions(package_id, version)
);

-- Installation/management jobs (independent of UI lifecycle).
CREATE TABLE installation_jobs (
    id           TEXT PRIMARY KEY,            -- job UUID
    kind         TEXT NOT NULL CHECK (kind IN ('install','uninstall','activate','profile_apply')),
    package_id   TEXT,
    version      TEXT,
    profile_id   TEXT,
    state        TEXT NOT NULL,               -- QUEUED..COMPLETED/FAILED/CANCELLED (installation-lifecycle.md §3)
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    finished_at  TEXT,
    error_code   TEXT                         -- stable code on failure (docs/ffi/bridge-contract.md)
);

-- Ordered structured job events (progress, stage changes, errors).
CREATE TABLE installation_events (
    job_id      TEXT NOT NULL REFERENCES installation_jobs(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,             -- monotonically increasing per job
    timestamp   TEXT NOT NULL,
    stage       TEXT NOT NULL,
    progress    REAL,                         -- 0.0..1.0 or NULL when indeterminate
    message     TEXT NOT NULL,                -- safe for display and logs
    error_code  TEXT,
    PRIMARY KEY (job_id, seq)
);

-- HTTP cache metadata for catalog responses.
CREATE TABLE catalog_cache (
    url            TEXT PRIMARY KEY,
    etag           TEXT,
    last_modified  TEXT,
    fetched_at     TEXT NOT NULL,
    response_json  TEXT NOT NULL
);

-- Developer profiles (user-editable; catalog profiles are cached through packages).
CREATE TABLE profiles (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL,
    origin       TEXT NOT NULL CHECK (origin IN ('catalog','user')),
    manifest     TEXT NOT NULL,               -- profile manifest JSON (schemas/profile.schema.json)
    updated_at   TEXT NOT NULL
);

-- Profile package membership.
CREATE TABLE profile_packages (
    profile_id     TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    package_id     TEXT NOT NULL,
    required       INTEGER NOT NULL,
    version_policy TEXT NOT NULL CHECK (version_policy IN ('exact','stable','latest','channel')),
    version        TEXT,                      -- required when policy = exact (enforced in validation)
    channel        TEXT,
    position       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (profile_id, package_id)
);

-- Application settings and consent state.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_installed_status   ON installed_versions(status);
CREATE INDEX idx_jobs_state         ON installation_jobs(state);
CREATE INDEX idx_jobs_package       ON installation_jobs(package_id);
CREATE INDEX idx_versions_package   ON package_versions(package_id);
CREATE INDEX idx_events_job         ON installation_events(job_id, seq);
