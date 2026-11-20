//! Entry types in the contract declaration order (SPEC §3.2; INTERFACE_CONTRACT §3, §9).
//!
//! The order of the variants of [`EntryTypes`] is the entry-type declaration order
//! and is part of the wire contract (TC-02). Do not reorder; append only.

use hdi::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::{BatchStatus, ContributionPlan, ContributionStatus, FundingState};

// 0. Enterprise
#[hdk_entry_helper]
#[derive(Clone)]
pub struct Enterprise {
    pub id: String,
    pub name: String,
    pub enterprise_type: String,
    pub owner_id: String,
    pub description: Option<String>,
    pub permalink: String,
    pub visible: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

// 1. FundingCampaign
#[hdk_entry_helper]
#[derive(Clone)]
pub struct FundingCampaign {
    pub id: String,
    pub enterprise_id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub goal_amount: String,
    pub current_amount: String,
    pub target_quantity: i64,
    pub fulfilled_quantity: i64,
    pub start_at: i64,
    pub end_at: i64,
    pub state: FundingState,
    pub created_at: i64,
    pub updated_at: i64,
}

// 2. FundingContribution
#[hdk_entry_helper]
#[derive(Clone)]
pub struct FundingContribution {
    pub id: String,
    pub campaign_id: String,
    pub contributor_id: String,
    pub quantity: i64,
    pub unit_price: String,
    pub total_amount: String,
    pub plan: ContributionPlan,
    pub paid_amount: String,
    pub installments_total: i64,
    pub installments_paid: i64,
    pub next_due_at: Option<i64>,
    pub created_at: i64,
    pub status: ContributionStatus,
    /// Owner of the campaign this contribution targets, captured at pledge time
    /// so integrity validation can authorise accept/decline without a link walk.
    pub campaign_owner_id: String,
}

// 3. FundingBatch
#[hdk_entry_helper]
#[derive(Clone)]
pub struct FundingBatch {
    pub id: String,
    pub campaign_id: String,
    pub owner_id: String,
    pub name: String,
    pub crop: String,
    pub variety: Option<String>,
    pub target_quantity: Option<String>,
    pub unit: Option<String>,
    pub sowing_date: Option<i64>,
    pub expected_harvest_at: Option<i64>,
    pub status: BatchStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

// 4. BatchReport
#[hdk_entry_helper]
#[derive(Clone)]
pub struct BatchReport {
    pub id: String,
    pub batch_id: String,
    pub report_date: i64,
    pub stage: String,
    pub health: String,
    pub expected_yield: Option<String>,
    pub actual_yield: Option<String>,
    pub notes: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

// 5. FundingProjection
#[hdk_entry_helper]
#[derive(Clone)]
pub struct FundingProjection {
    pub id: String,
    pub campaign_id: String,
    pub owner_id: String,
    pub periods: i64,
    pub start_month: i64,
    pub target_margin_pct: String,
    pub notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// 6. ProjectionLine
#[hdk_entry_helper]
#[derive(Clone)]
pub struct ProjectionLine {
    pub id: String,
    /// Parent projection's action address (base64 string). See README § deviations.
    pub projection_id: String,
    pub period_index: i64,
    pub category: String,
    pub description: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub unit_cost: Option<String>,
    pub amount: String,
    pub sort_order: i64,
    pub created_at: i64,
}

// 7. PickupLocation
#[hdk_entry_helper]
#[derive(Clone)]
pub struct PickupLocation {
    pub id: String,
    pub enterprise_id: String,
    pub owner_id: String,
    pub name: String,
    pub address: Option<String>,
    pub instructions: Option<String>,
    pub created_at: i64,
}

// 8. Profile
#[hdk_entry_helper]
#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub created_at: i64,
}

// 9. FarmersCluster
#[hdk_entry_helper]
#[derive(Clone)]
pub struct FarmersCluster {
    pub id: String,
    pub name: String,
    pub region: String,
    pub governance: String,
    pub season: String,
    pub currency: String,
    pub owner_id: String,
    pub member_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Entry types in declaration order (INTERFACE_CONTRACT §3).
#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
#[derive(Serialize, Deserialize)]
pub enum EntryTypes {
    Enterprise(Enterprise),
    FundingCampaign(FundingCampaign),
    FundingContribution(FundingContribution),
    FundingBatch(FundingBatch),
    BatchReport(BatchReport),
    FundingProjection(FundingProjection),
    ProjectionLine(ProjectionLine),
    PickupLocation(PickupLocation),
    Profile(Profile),
    FarmersCluster(FarmersCluster),
}