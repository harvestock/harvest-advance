//! Client signals (SPEC §5.10; INTERFACE_CONTRACT §11).

use holochain_serialized_bytes::prelude::*;

/// Wire shape: one object keyed by the variant name, e.g.
/// `{"CampaignCreated": {"campaign_id": "..."}}`.
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub enum Signal {
    CampaignCreated {
        campaign_id: String,
    },
    CampaignStateChanged {
        campaign_id: String,
        state: String,
    },
    ContributionMade {
        campaign_id: String,
        contributor_id: String,
        amount: String,
    },
    BatchReportPosted {
        batch_id: String,
        report_id: String,
    },
    // Declared per INTERFACE_CONTRACT §11 but intentionally never emitted
    // (TC-59).
    CampaignFunded {
        campaign_id: String,
        total_amount: String,
    },
    BatchStatusChanged {
        batch_id: String,
        status: String,
    },
}