//! Catalog browsing without a network (P1-04) and with remote
//! revalidation (P2-04).
//!
//! - [`fixtures`]: loads committed fixture manifests (package/profile
//!   documents conforming to the JSON schemas) from a directory.
//! - [`offline`]: `OfflineCatalog` seeds the SQLite cache from fixtures and
//!   serves all browsing queries from the cache only, so the catalog is
//!   fully browsable offline after the first load (P1-04 acceptance).
//! - [`remote`]: `RemoteCatalog` revalidates the same cache against the
//!   public catalog API with ETag conditional requests and degrades to
//!   cache-only browsing whenever the network is down (P2-04).
//!
//! No network access happens outside `remote::ReqwestCatalogHttp`; the
//! transport is injectable so tests never touch the network.

pub mod error;
pub mod fixtures;
pub mod offline;
pub mod remote;

pub use error::CatalogError;
pub use fixtures::FixtureCatalog;
pub use offline::{OfflineCatalog, SyncReport};
pub use remote::{
    CatalogFetch, CatalogHttp, HttpError, RemoteCatalog, RemoteSyncReport, ReqwestCatalogHttp,
};
