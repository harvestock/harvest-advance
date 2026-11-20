//! State machines and enumerations (SPEC §3.3, §4.1, §4.2; INTERFACE_CONTRACT §6, §7).

pub const FUNDING_STATES: [&str; 6] = [
    "Draft",
    "Active",
    "Funded",
    "Completed",
    "Canceled",
    "Failed",
];

pub const BATCH_STATUSES: [&str; 6] = [
    "Planned",
    "Growing",
    "Ready",
    "Packed",
    "Delivered",
    "Failed",
];

pub const CONTRIBUTION_PLANS: [&str; 2] = ["Full", "Installments"];

pub fn is_funding_state(value: &str) -> bool {
    FUNDING_STATES.contains(&value)
}

pub fn is_batch_status(value: &str) -> bool {
    BATCH_STATUSES.contains(&value)
}

pub fn is_contribution_plan(value: &str) -> bool {
    CONTRIBUTION_PLANS.contains(&value)
}

/// Campaign transition table (SPEC §4.1, extended with failure paths).
///
/// ```text
/// Draft -> Active -> Funded -> Completed
///            |          |
///            +-> Canceled <-+
/// Active/Funded -> Failed (failure to deliver)
/// Completed -> Completed (idempotent)
/// ```
pub fn campaign_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("Draft", "Active")
            | ("Active", "Funded")
            | ("Active", "Canceled")
            | ("Active", "Failed")
            | ("Funded", "Completed")
            | ("Funded", "Canceled")
            | ("Funded", "Failed")
            | ("Completed", "Completed")
    )
}

/// Contribution lifecycle transition table.
///
/// ```text
/// Pending -> Accepted | Declined | Defaulted
/// Accepted -> Completed | Defaulted
/// (Completed / Declined / Defaulted are terminal)
/// ```
pub fn contribution_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("Pending", "Accepted")
            | ("Pending", "Declined")
            | ("Pending", "Defaulted")
            | ("Accepted", "Completed")
            | ("Accepted", "Defaulted")
            | ("Completed", "Completed")
    )
}

/// Batch transition table (SPEC §4.2).
pub fn batch_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("Planned", "Growing")
            | ("Growing", "Ready")
            | ("Ready", "Packed")
            | ("Packed", "Delivered")
            | ("Growing", "Failed")
            | ("Ready", "Failed")
            | ("Packed", "Failed")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_transitions() {
        for (from, to) in [
            ("Draft", "Active"),
            ("Active", "Funded"),
            ("Active", "Canceled"),
            ("Funded", "Completed"),
            ("Funded", "Canceled"),
            ("Completed", "Completed"),
        ] {
            assert!(campaign_allowed(from, to), "{from}->{to} should be allowed");
        }

        // A representative sample of the 30 invalid transitions.
        for (from, to) in [
            ("Draft", "Funded"),
            ("Draft", "Completed"),
            ("Active", "Completed"),
            ("Completed", "Active"),
            ("Canceled", "Active"),
            ("Failed", "Active"),
            ("Funded", "Funded"),
        ] {
            assert!(!campaign_allowed(from, to), "{from}->{to} should be rejected");
        }
    }

    #[test]
    fn batch_transitions() {
        for (from, to) in [
            ("Planned", "Growing"),
            ("Growing", "Ready"),
            ("Ready", "Packed"),
            ("Packed", "Delivered"),
            ("Growing", "Failed"),
            ("Ready", "Failed"),
            ("Packed", "Failed"),
        ] {
            assert!(batch_allowed(from, to), "{from}->{to} should be allowed");
        }

        for (from, to) in [
            ("Planned", "Ready"),
            ("Delivered", "Packed"),
            ("Failed", "Planned"),
            ("Planned", "Delivered"),
        ] {
            assert!(!batch_allowed(from, to), "{from}->{to} should be rejected");
        }
    }

    #[test]
    fn enum_membership() {
        assert!(is_funding_state("Active"));
        assert!(!is_funding_state("active"));
        assert!(is_batch_status("Delivered"));
        assert!(is_contribution_plan("Installments"));
    }

    #[test]
    fn campaign_failure_paths() {
        assert!(campaign_allowed("Active", "Failed"));
        assert!(campaign_allowed("Funded", "Failed"));
        assert!(!campaign_allowed("Draft", "Failed"));
        assert!(!campaign_allowed("Failed", "Active"));
    }

    #[test]
    fn contribution_transitions() {
        for (from, to) in [
            ("Pending", "Accepted"),
            ("Pending", "Declined"),
            ("Pending", "Defaulted"),
            ("Accepted", "Completed"),
            ("Accepted", "Defaulted"),
            ("Completed", "Completed"),
        ] {
            assert!(contribution_allowed(from, to), "{from}->{to} should be allowed");
        }

        for (from, to) in [
            ("Accepted", "Accepted"),
            ("Declined", "Accepted"),
            ("Defaulted", "Accepted"),
            ("Completed", "Accepted"),
            ("Declined", "Completed"),
            ("Defaulted", "Completed"),
        ] {
            assert!(!contribution_allowed(from, to), "{from}->{to} should be rejected");
        }
    }
}