//! First bridge operations (P1-02): `get_app_info` and `get_system_info`.
//!
//! These validate the toolchain end to end (Rust → codegen → Dart) and serve
//! the About/diagnostics surfaces (DESIGN.md §10). Catalog and installation
//! operations land in P1-04/P1-05 and follow docs/ffi/bridge-contract.md.

use std::env::consts::{ARCH, OS};

/// Identity of the running engine, for About / diagnostics.
#[derive(Debug, Clone)]
pub struct AppInfo {
    /// Engine version, sourced from the crate version.
    pub version: String,
    /// Human-readable engine name.
    pub engine_name: String,
}

/// Host system facts used for platform gating (which installers the UI may
/// offer, minimum-OS messaging, etc.).
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// Target OS triple component, e.g. "windows".
    pub os: String,
    /// CPU architecture, e.g. "x86_64".
    pub arch: String,
    /// Engine capability flags exposed to the UI. Keeps Flutter from
    /// hard-coding feature detection that belongs to Rust.
    pub capabilities: Vec<String>,
}

/// Application/engine information for the About screen and diagnostics.
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        engine_name: "DevKit Engine".to_string(),
    }
}

/// Host OS/architecture/capability snapshot for platform gating.
pub fn get_system_info() -> SystemInfo {
    SystemInfo {
        os: OS.to_string(),
        arch: ARCH.to_string(),
        capabilities: vec!["sqlite_registry".to_string(), "catalog_client".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_crate_version() {
        let info = get_app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.engine_name.is_empty());
    }

    #[test]
    fn system_info_reports_host_os_and_arch() {
        let info = get_system_info();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(info.capabilities.iter().all(|c| !c.is_empty()));
    }
}
