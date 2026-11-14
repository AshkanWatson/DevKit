//! Job stages and the state machine
//! (docs/architecture/installation-lifecycle.md §3, bridge-contract.md
//! § Event model).

/// One stage of the installation lifecycle. Serialized to its documented
/// SCREAMING_SNAKE_CASE wire form (`"DOWNLOADING"`, `"ROLLED_BACK"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Queued,
    Resolving,
    Preparing,
    Downloading,
    Verifying,
    Extracting,
    Installing,
    Configuring,
    Validating,
    Completed,
    Failed,
    Cancelled,
    Paused,
    RollingBack,
    RolledBack,
}

impl Stage {
    pub const ALL: &'static [Stage] = &[
        Stage::Queued,
        Stage::Resolving,
        Stage::Preparing,
        Stage::Downloading,
        Stage::Verifying,
        Stage::Extracting,
        Stage::Installing,
        Stage::Configuring,
        Stage::Validating,
        Stage::Completed,
        Stage::Failed,
        Stage::Cancelled,
        Stage::Paused,
        Stage::RollingBack,
        Stage::RolledBack,
    ];

    /// Wire form used in events and in the `installation_jobs.state`
    /// column (installation-lifecycle.md §3 table).
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Queued => "QUEUED",
            Stage::Resolving => "RESOLVING",
            Stage::Preparing => "PREPARING",
            Stage::Downloading => "DOWNLOADING",
            Stage::Verifying => "VERIFYING",
            Stage::Extracting => "EXTRACTING",
            Stage::Installing => "INSTALLING",
            Stage::Configuring => "CONFIGURING",
            Stage::Validating => "VALIDATING",
            Stage::Completed => "COMPLETED",
            Stage::Failed => "FAILED",
            Stage::Cancelled => "CANCELLED",
            Stage::Paused => "PAUSED",
            Stage::RollingBack => "ROLLING_BACK",
            Stage::RolledBack => "ROLLED_BACK",
        }
    }

    /// Parse the wire form.
    pub fn parse(value: &str) -> Option<Stage> {
        Stage::ALL.iter().copied().find(|s| s.as_str() == value)
    }

    /// Terminal states per bridge-contract.md § Cancellation semantics:
    /// they reject further cancellation. (`FAILED` and `CANCELLED` still
    /// transition to `ROLLING_BACK` — the rollback review, rule §3.2 — but
    /// no further user action applies to them.)
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Stage::Completed | Stage::Failed | Stage::Cancelled | Stage::RolledBack
        )
    }

    /// Whether the transition `self -> to` is allowed by the state machine.
    /// `FAILED` and `CANCELLED` are reachable from every active stage
    /// ("unrecoverable error at any stage", "honored at a safe boundary").
    pub fn can_transition(self, to: Stage) -> bool {
        use Stage::*;
        match self {
            Queued => matches!(to, Resolving | Failed | Cancelled),
            Resolving => matches!(to, Preparing | Failed | Cancelled),
            Preparing => matches!(to, Downloading | Failed | Cancelled),
            // Pause only where safe resume is supported (download stage).
            Downloading => matches!(to, Verifying | Paused | Failed | Cancelled),
            Paused => matches!(to, Downloading | Failed | Cancelled),
            // EXTRACTING covers the archive adapter; INSTALLING the
            // installer adapters (both reach CONFIGURING).
            Verifying => matches!(to, Extracting | Installing | Failed | Cancelled),
            Extracting => matches!(to, Configuring | Failed | Cancelled),
            Installing => matches!(to, Configuring | Failed | Cancelled),
            Configuring => matches!(to, Validating | Failed | Cancelled),
            Validating => matches!(to, Completed | Failed | Cancelled),
            Failed => matches!(to, RollingBack),
            Cancelled => matches!(to, RollingBack),
            RollingBack => matches!(to, RolledBack),
            Completed | RolledBack => false,
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_forms_are_stable() {
        assert_eq!(Stage::Queued.as_str(), "QUEUED");
        assert_eq!(Stage::RollingBack.as_str(), "ROLLING_BACK");
        assert_eq!(Stage::RolledBack.as_str(), "ROLLED_BACK");
        assert_eq!(Stage::parse("DOWNLOADING"), Some(Stage::Downloading));
        assert_eq!(Stage::parse("downloading"), None);
    }

    #[test]
    fn happy_path_chain_is_allowed() {
        let chain = [
            Stage::Queued,
            Stage::Resolving,
            Stage::Preparing,
            Stage::Downloading,
            Stage::Verifying,
            Stage::Extracting,
            Stage::Configuring,
            Stage::Validating,
            Stage::Completed,
        ];
        for pair in chain.windows(2) {
            assert!(
                pair[0].can_transition(pair[1]),
                "{} -> {} must be allowed",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn rollback_chain_is_allowed_and_terminal_states_stop() {
        assert!(Stage::Failed.can_transition(Stage::RollingBack));
        assert!(Stage::Cancelled.can_transition(Stage::RollingBack));
        assert!(Stage::RollingBack.can_transition(Stage::RolledBack));
        for terminal in [Stage::Completed, Stage::RolledBack] {
            for to in Stage::ALL {
                assert!(
                    !terminal.can_transition(*to),
                    "{terminal} must have no outgoing transitions"
                );
            }
        }
    }

    #[test]
    fn failure_and_cancellation_are_reachable_from_active_stages() {
        for from in Stage::ALL {
            if from.is_terminal()
                || matches!(
                    from,
                    Stage::Failed | Stage::Cancelled | Stage::RollingBack | Stage::RolledBack
                )
            {
                continue;
            }
            assert!(
                from.can_transition(Stage::Failed),
                "{from} must reach FAILED"
            );
            assert!(
                from.can_transition(Stage::Cancelled),
                "{from} must reach CANCELLED"
            );
        }
    }

    #[test]
    fn pause_is_download_only() {
        assert!(Stage::Downloading.can_transition(Stage::Paused));
        assert!(Stage::Paused.can_transition(Stage::Downloading));
        for from in Stage::ALL {
            if !matches!(from, Stage::Downloading | Stage::Paused) {
                assert!(!from.can_transition(Stage::Paused), "{from} -> PAUSED");
            }
        }
    }
}
