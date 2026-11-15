//! Installation jobs (P1-05): real state machine, event stream, and
//! persistence with artificial work instead of downloads.
//!
//! Authoritative behavior:
//! - State machine: docs/architecture/installation-lifecycle.md §3.
//! - Event model + cancellation semantics: docs/ffi/bridge-contract.md.
//! - Persistence: `installation_jobs` / `installation_events`
//!   (docs/architecture/local-database.md §2).
//!
//! Jobs run on their own threads (or synchronously in tests) and are
//! independent of any UI lifecycle: the UI re-attaches by replaying events
//! from SQLite. This crate has no Flutter dependency, performs no network
//! or filesystem access, and runs no external processes — the "work" is a
//! deterministic simulated plan ([`simulated::SimulatedPlan`]) that stands
//! in for the Phase 3 installer adapters.

pub mod environment;
pub mod error;
pub mod events;
pub mod manager;
pub mod pipeline;
pub mod runner;
pub mod simulated;
pub mod state;
pub mod store;

pub use environment::{
    activate, default_path_environment, plan_activation, remove_shims_referencing,
    ActivationConsent, ActivationOutcome, ActivationPlan, PathChange, PathEnvironment, ShimPlan,
};
pub use error::JobsError;
pub use events::JobEvent;
pub use manager::{CancelOutcome, JobManager};
pub use pipeline::{
    cleanup_abandoned, current_platform, uninstall_version, validate_installation, InstallReceipt,
    InstallSource, RealInstallConfig, ReceiptFile, RegistrySource, StageFailure, UninstallOutcome,
};
pub use simulated::{RunOutcome, SimulatedPlan};
pub use state::Stage;
pub use store::{JobKind, JobSnapshot, JobStore};
