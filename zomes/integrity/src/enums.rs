//! Enumerations with exact wire names and order (SPEC §3.3; INTERFACE_CONTRACT §6).

use holochain_serialized_bytes::prelude::*;

/// Campaign lifecycle state.
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundingState {
    Draft,
    Active,
    Funded,
    Completed,
    Canceled,
    Failed,
}

impl FundingState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FundingState::Draft => "Draft",
            FundingState::Active => "Active",
            FundingState::Funded => "Funded",
            FundingState::Completed => "Completed",
            FundingState::Canceled => "Canceled",
            FundingState::Failed => "Failed",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Draft" => Some(FundingState::Draft),
            "Active" => Some(FundingState::Active),
            "Funded" => Some(FundingState::Funded),
            "Completed" => Some(FundingState::Completed),
            "Canceled" => Some(FundingState::Canceled),
            "Failed" => Some(FundingState::Failed),
            _ => None,
        }
    }
}

/// Batch lifecycle state.
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Planned,
    Growing,
    Ready,
    Packed,
    Delivered,
    Failed,
}

impl BatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchStatus::Planned => "Planned",
            BatchStatus::Growing => "Growing",
            BatchStatus::Ready => "Ready",
            BatchStatus::Packed => "Packed",
            BatchStatus::Delivered => "Delivered",
            BatchStatus::Failed => "Failed",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Planned" => Some(BatchStatus::Planned),
            "Growing" => Some(BatchStatus::Growing),
            "Ready" => Some(BatchStatus::Ready),
            "Packed" => Some(BatchStatus::Packed),
            "Delivered" => Some(BatchStatus::Delivered),
            "Failed" => Some(BatchStatus::Failed),
            _ => None,
        }
    }
}

/// Contribution payment plan.
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContributionPlan {
    Full,
    Installments,
}

impl ContributionPlan {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContributionPlan::Full => "Full",
            ContributionPlan::Installments => "Installments",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Full" => Some(ContributionPlan::Full),
            "Installments" => Some(ContributionPlan::Installments),
            _ => None,
        }
    }
}

/// Contribution lifecycle state (pledge decision + payment outcome).
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContributionStatus {
    Pending,
    Accepted,
    Declined,
    Defaulted,
    Completed,
}

impl ContributionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContributionStatus::Pending => "Pending",
            ContributionStatus::Accepted => "Accepted",
            ContributionStatus::Declined => "Declined",
            ContributionStatus::Defaulted => "Defaulted",
            ContributionStatus::Completed => "Completed",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "Pending" => Some(ContributionStatus::Pending),
            "Accepted" => Some(ContributionStatus::Accepted),
            "Declined" => Some(ContributionStatus::Declined),
            "Defaulted" => Some(ContributionStatus::Defaulted),
            "Completed" => Some(ContributionStatus::Completed),
            _ => None,
        }
    }
}