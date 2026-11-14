//! HTTP cache metadata for catalog responses
//! (docs/architecture/local-database.md §2, table `catalog_cache`).
//!
//! Keyed by URL; stores the validators (ETag / Last-Modified) needed for
//! conditional requests plus the last response body so browsing works
//! offline (P2-04).

use rusqlite::{params, Connection};

use crate::error::RegistryError;

/// One cached catalog HTTP response.
#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetched_at: String,
    pub response_json: String,
}

pub struct CatalogHttpCache<'a> {
    conn: &'a Connection,
}

impl<'a> CatalogHttpCache<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace the cache entry for `url`. `fetched_at` is the
    /// time of this write (UTC ISO-8601).
    pub fn put(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        response_json: &str,
    ) -> Result<(), RegistryError> {
        let fetched_at = crate::error::now_utc_rfc3339()?;
        self.conn.execute(
            "INSERT INTO catalog_cache (url, etag, last_modified, fetched_at, response_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(url) DO UPDATE SET
                etag = excluded.etag,
                last_modified = excluded.last_modified,
                fetched_at = excluded.fetched_at,
                response_json = excluded.response_json",
            params![url, etag, last_modified, fetched_at, response_json],
        )?;
        Ok(())
    }

    /// Load the cache entry for `url`, if present.
    pub fn get(&self, url: &str) -> Result<Option<CachedResponse>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT url, etag, last_modified, fetched_at, response_json
             FROM catalog_cache WHERE url = ?1",
        )?;
        let mut rows = stmt.query_map(params![url], |row| {
            Ok(CachedResponse {
                url: row.get(0)?,
                etag: row.get(1)?,
                last_modified: row.get(2)?,
                fetched_at: row.get(3)?,
                response_json: row.get(4)?,
            })
        })?;
        match rows.next() {
            None => Ok(None),
            Some(row) => Ok(Some(row?)),
        }
    }

    /// Remove the cache entry for `url`.
    pub fn remove(&self, url: &str) -> Result<(), RegistryError> {
        self.conn
            .execute("DELETE FROM catalog_cache WHERE url = ?1", params![url])?;
        Ok(())
    }
}
