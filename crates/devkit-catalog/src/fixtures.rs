//! Fixture manifest loading (no network).
//!
//! A fixture directory contains package and/or profile manifest documents
//! (`schemas/package.schema.json`, `schemas/profile.schema.json`). Every
//! document is parsed and semantically validated by the P1-03 loader, so a
//! corrupt fixture fails loudly instead of poisoning the cache.

use std::fs;
use std::path::Path;

use devkit_core::manifest::{PackageManifest, ProfileManifest};

use crate::error::CatalogError;

/// All valid manifests found in a fixture directory.
#[derive(Debug, Clone)]
pub struct FixtureCatalog {
    pub packages: Vec<PackageManifest>,
    pub profiles: Vec<ProfileManifest>,
}

impl FixtureCatalog {
    /// Load and validate every `*.json` document in `dir` (one document per
    /// file, files sorted by name for deterministic ordering). A document is
    /// a profile when it has a `packages` array (profiles) at the top level;
    /// otherwise it is a package manifest. Any invalid file is an error.
    pub fn load_dir(dir: &Path) -> Result<Self, CatalogError> {
        let mut names: Vec<String> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            names.push(
                path.file_name()
                    .expect("dir entry with a file path")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        names.sort();

        let mut packages = Vec::new();
        let mut profiles = Vec::new();
        for name in names {
            let path = dir.join(&name);
            let json = fs::read_to_string(&path)?;
            let value: serde_json::Value = serde_json::from_str(&json)?;
            if value.get("packages").is_some() {
                profiles.push(ProfileManifest::from_json(&json)?);
            } else {
                packages.push(PackageManifest::from_json(&json)?);
            }
        }
        Ok(Self { packages, profiles })
    }
}
