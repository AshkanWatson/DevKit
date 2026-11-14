//! Property-style tests for archive entry-path validation
//! (testing-strategy.md "Property/fuzz" row, P3-02 acceptance criteria).
//!
//! Properties:
//! 1. Any path built from benign components validates, joins strictly
//!    under the staging root, and contains only normal components.
//! 2. Injecting a `..` component anywhere into such a path is always
//!    rejected.
//! 3. A leading separator or drive letter is always rejected.

use std::path::{Component, Path};

use devkit_install::{ensure_within, validate_entry_path, ArchiveError, UnsafePathReason};
use proptest::prelude::*;

/// Components that would be rejected for reserved-name reasons — those
/// are covered by the dedicated unit tests; the properties below isolate
/// traversal/absolute semantics.
fn is_reserved(component: &str) -> bool {
    let stem = component
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(component);
    [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ]
    .iter()
    .any(|name| name.eq_ignore_ascii_case(stem))
}

prop_compose! {
    fn benign_entry()(components in proptest::collection::vec("[a-zA-Z0-9_]{1,8}", 1..6)) -> Vec<String> {
        components
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn benign_paths_validate_and_stay_under_the_staging_root(components in benign_entry()) {
        // Skip components that hit the reserved-name policy (own tests).
        prop_assume!(!components.iter().any(|c| is_reserved(c)));
        let entry = components.join("/");
        let relative = validate_entry_path(&entry).unwrap_or_else(|error| {
            panic!("benign entry {entry:?} must validate, got {error}")
        });
        for component in relative.components() {
            prop_assert!(matches!(component, Component::Normal(_)), "{relative:?}");
        }
        let staging = Path::new("S:/staging/pkg-1.0");
        ensure_within(staging, &staging.join(&relative))
            .unwrap_or_else(|error| panic!("validated {entry:?} escaped the root: {error}"));
    }

    #[test]
    fn injected_parent_traversal_is_always_rejected(
        components in benign_entry(),
        position in proptest::num::usize::ANY,
    ) {
        prop_assume!(!components.iter().any(|c| is_reserved(c)));
        let mut hostile = components.clone();
        let at = position % (hostile.len() + 1);
        hostile.insert(at, "..".to_owned());
        let entry = hostile.join("/");
        prop_assert!(validate_entry_path(&entry).is_err(), "accepted {entry:?}");
    }

    #[test]
    fn leading_separator_is_always_rejected(components in benign_entry()) {
        prop_assume!(!components.iter().any(|c| is_reserved(c)));
        let entry = format!("/{}", components.join("/"));
        match validate_entry_path(&entry) {
            Err(ArchiveError::UnsafeEntryPath { reason: UnsafePathReason::Absolute, .. }) => {}
            other => panic!("leading separator accepted: {entry:?} → {other:?}"),
        }
    }

    #[test]
    fn drive_letter_is_always_rejected(
        components in benign_entry(),
        letter in "[a-z]",
    ) {
        prop_assume!(!components.iter().any(|c| is_reserved(c)));
        let entry = format!("{letter}:{}", if components.is_empty() { String::new() } else { format!("/{}", components.join("/")) });
        prop_assert!(validate_entry_path(&entry).is_err(), "accepted {entry:?}");
    }

    #[test]
    fn backslash_form_of_traversal_is_rejected(components in benign_entry()) {
        prop_assume!(!components.iter().any(|c| is_reserved(c)));
        let mut hostile = components.clone();
        hostile.insert(0, "..".to_owned());
        let entry = hostile.join("\\");
        prop_assert!(validate_entry_path(&entry).is_err(), "accepted {entry:?}");
    }
}
