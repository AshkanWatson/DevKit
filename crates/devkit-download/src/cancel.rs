//! Cooperative cancellation for the download engine.
//!
//! The engine checks the token at every chunk boundary
//! (installation-lifecycle.md §3 transition rule 3) and stops promptly.
//! P3-03 will bridge this to the job runner's cancellation; for now it is
//! a plain shared flag — no timers, no background threads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared, cheap cancellation flag.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Requests cancellation (safe to call multiple times).
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
