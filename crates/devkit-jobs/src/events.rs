//! Structured job events (docs/ffi/bridge-contract.md § Event model).

/// One entry of a job's event log. Field semantics follow the contract:
/// 1-based `seq` per job, UTC ISO-8601 timestamp, optional progress
/// fraction (absent = indeterminate — the UI must never show a fake
/// percentage), safe `message`, optional stable error code.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JobEvent {
    pub job_id: String,
    /// Monotonically increasing, 1-based, per job.
    pub seq: i64,
    /// UTC ISO-8601 (RFC 3339).
    pub timestamp: String,
    /// Stage wire form (`"DOWNLOADING"`, ...).
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Safe for display and logs.
    pub message: String,
    /// Stable code string (`"NETWORK_ERROR"`, ...) when the event
    /// represents a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}
