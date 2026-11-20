//! Link types in the contract declaration order (SPEC §3.4; INTERFACE_CONTRACT §4).
//!
//! The order of the variants of [`LinkTypes`] is the link-type index (0–10) and
//! is part of the wire contract (TC-03). Do not reorder; append only.

use hdi::prelude::*;

#[hdk_link_types]
pub enum LinkTypes {
    // 0
    EnterpriseCampaign,
    // 1
    CampaignContribution,
    // 2
    CampaignBatch,
    // 3
    BatchReport,
    // 4
    CampaignProjection,
    // 5
    ProfileContribution,
    // 6
    EnterpriseLocation,
    // 7
    CampaignProduct,
    // 8
    EnterpriseProfile,
    // 9
    ClusterList,
    // 10
    CampaignCatalog,
}