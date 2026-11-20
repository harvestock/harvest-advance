//! Harvest Advance coordinator zome.
//!
//! Implements every function in INTERFACE_CONTRACT §8, plus the signal surface
//! of §11 and a small number of documented extra reads.

use std::cmp::Ordering;
use std::str::FromStr;

use hdk::prelude::*;
use harvest_advance_integrity::*;
use harvest_advance_logic::{rules, transitions, Decimal};
use holo_hash::{ActionHash, ActionHashB64, AgentPubKeyB64};

mod anchors;
mod ids;
mod records;
mod signals;

use anchors::*;
use ids::new_id;
use records::latest_record;
use signals::Signal;

// Wire the coordinator to the integrity zome's entry and link types.
#[hdk_dependent_entry_types]
pub enum EntryZomes {
    Integrity(EntryTypes),
}

#[hdk_dependent_link_types]
pub enum LinkZomes {
    Integrity(LinkTypes),
}

fn guest_error<T>(message: impl Into<String>) -> ExternResult<T> {
    Err(wasm_error!(WasmErrorInner::Guest(message.into())))
}

fn caller() -> ExternResult<AgentPubKey> {
    Ok(agent_info()?.agent_initial_pubkey)
}

/// Agent identity rendered as a base64 string (SPEC §3.1).
fn agent_b64(key: &AgentPubKey) -> String {
    AgentPubKeyB64::from(key.clone()).to_string()
}

fn now_secs() -> ExternResult<i64> {
    Ok(sys_time()?.as_seconds_and_nanos().0)
}

/// Resolve the owner of an enterprise by reading its anchor, if it exists.
fn resolve_enterprise_owner(enterprise_id: &str) -> ExternResult<Option<String>> {
    let base = enterprise_anchor(enterprise_id)?;
    for link in links_from(&base, LinkTypes::EnterpriseProfile)? {
        if let Some(action_hash) = action_hash_of(&link.target) {
            if let Some(record) = latest_record(&action_hash)? {
                if let Ok(enterprise) = Enterprise::try_from(&record) {
                    return Ok(Some(enterprise.owner_id));
                }
            }
        }
    }
    Ok(None)
}

/// Resolve the owner of a campaign by reading its anchor, if it exists.
fn resolve_campaign_owner(campaign_id: &str) -> ExternResult<Option<String>> {
    let base = campaign_anchor(campaign_id)?;
    for link in links_from(&base, LinkTypes::EnterpriseCampaign)? {
        if let Some(action_hash) = action_hash_of(&link.target) {
            if let Some(record) = latest_record(&action_hash)? {
                if let Ok(campaign) = FundingCampaign::try_from(&record) {
                    if campaign.id == campaign_id {
                        return Ok(Some(campaign.owner_id));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Find a campaign by its 32-hex id via the campaign anchor.
fn find_campaign(campaign_id: &str) -> ExternResult<Option<(ActionHash, FundingCampaign)>> {
    let base = campaign_anchor(campaign_id)?;
    for link in links_from(&base, LinkTypes::EnterpriseCampaign)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        if let Some(record) = latest_record(&action_hash)? {
            if let Ok(campaign) = FundingCampaign::try_from(&record) {
                if campaign.id == campaign_id {
                    return Ok(Some((record.action_address().clone(), campaign)));
                }
            }
        }
    }
    Ok(None)
}

/// Find a contribution by its 32-hex id under a campaign.
fn find_contribution(
    campaign_id: &str,
    contribution_id: &str,
) -> ExternResult<Option<(ActionHash, FundingContribution)>> {
    for link in links_from(&campaign_anchor(campaign_id)?, LinkTypes::CampaignContribution)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        if let Some(record) = latest_record(&action_hash)? {
            if let Ok(contribution) = FundingContribution::try_from(&record) {
                if contribution.id == contribution_id {
                    return Ok(Some((record.action_address().clone(), contribution)));
                }
            }
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

#[hdk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
    Ok(InitCallbackResult::Pass)
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct CreateProfileInput {
    pub email: String,
    pub name: Option<String>,
}

#[hdk_extern]
fn create_profile(input: CreateProfileInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_profile(&input.email).map_err(|message| wasm_error!(message))?;

    // One profile per agent (TC-29). The profile anchor is shared with
    // contribution reads (link type 5); profile links are distinguished by
    // their target entry type.
    let base = profile_anchor(&author)?;
    for link in links_from(&base, LinkTypes::ProfileContribution)? {
        if let Some(action_hash) = action_hash_of(&link.target) {
            if let Some(record) = latest_record(&action_hash)? {
                if Profile::try_from(&record).is_ok() {
                    return guest_error("a profile already exists for this agent");
                }
            }
        }
    }

    let profile = Profile {
        id: agent_b64(&author),
        email: input.email,
        name: input.name,
        created_at: now,
    };

    let action_hash = create_entry(EntryTypes::Profile(profile.clone()))?;
    create_link(
        base,
        action_hash.clone(),
        LinkTypes::ProfileContribution,
        LinkTag::new(Vec::new()),
    )?;

    Ok(action_hash.into())
}

// ---------------------------------------------------------------------------
// Enterprises
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct CreateEnterpriseInput {
    pub name: String,
    pub enterprise_type: String,
    pub description: Option<String>,
    pub permalink: String,
    pub visible: bool,
}

#[hdk_extern]
fn create_enterprise(input: CreateEnterpriseInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_enterprise(&input.name, &input.permalink)
        .map_err(|message| wasm_error!(message))?;

    let id = new_id()?;
    let enterprise = Enterprise {
        id: id.clone(),
        name: input.name,
        enterprise_type: input.enterprise_type,
        owner_id: agent_b64(&author),
        description: input.description,
        permalink: input.permalink,
        visible: input.visible,
        created_at: now,
        updated_at: now,
    };

    let action_hash = create_entry(EntryTypes::Enterprise(enterprise))?;
    create_link(
        enterprise_anchor(&id)?,
        action_hash.clone(),
        LinkTypes::EnterpriseProfile,
        LinkTag::new(Vec::new()),
    )?;

    Ok(action_hash.into())
}

// ---------------------------------------------------------------------------
// Campaigns
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct CreateCampaignInput {
    pub enterprise_id: String,
    /// Accepted and ignored (SPEC §5.3); retained for client compatibility.
    pub product_variant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub goal_amount: String,
    pub target_quantity: i64,
    pub start_at: i64,
    pub end_at: i64,
}

#[hdk_extern]
fn create_campaign(input: CreateCampaignInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_campaign(
        &input.name,
        &input.enterprise_id,
        &input.goal_amount,
        input.target_quantity,
        input.start_at,
        input.end_at,
    )
    .map_err(|message| wasm_error!(message))?;

    // Owner resolution: enterprise owner, else the caller (SPEC §5.3).
    let owner_id = resolve_enterprise_owner(&input.enterprise_id)?.unwrap_or_else(|| agent_b64(&author));

    let id = new_id()?;
    let campaign = FundingCampaign {
        id: id.clone(),
        enterprise_id: input.enterprise_id,
        owner_id,
        name: input.name,
        description: input.description,
        goal_amount: input.goal_amount,
        current_amount: "0".to_string(),
        target_quantity: input.target_quantity,
        fulfilled_quantity: 0,
        start_at: input.start_at,
        end_at: input.end_at,
        state: FundingState::Draft,
        created_at: now,
        updated_at: now,
    };

    let action_hash = create_entry(EntryTypes::FundingCampaign(campaign.clone()))?;

    // enterprise/{enterprise_id} -> campaign (type 0)
    create_link(
        enterprise_anchor(&campaign.enterprise_id)?,
        action_hash.clone(),
        LinkTypes::EnterpriseCampaign,
        LinkTag::new(Vec::new()),
    )?;
    // campaigns -> campaign (type 10)
    create_link(
        campaigns_anchor()?,
        action_hash.clone(),
        LinkTypes::CampaignCatalog,
        LinkTag::new(Vec::new()),
    )?;
    // campaign/{new_id} -> campaign (type 0)
    create_link(
        campaign_anchor(&campaign.id)?,
        action_hash.clone(),
        LinkTypes::EnterpriseCampaign,
        LinkTag::new(Vec::new()),
    )?;

    emit_signal(Signal::CampaignCreated { campaign_id: id })?;

    Ok(action_hash.into())
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct ListCampaignsInput {
    pub state: Option<String>,
    pub enterprise_id: Option<String>,
    pub limit: Option<i64>,
}

#[hdk_extern]
fn list_campaigns(
    input: ListCampaignsInput,
) -> ExternResult<Vec<(ActionHashB64, FundingCampaign)>> {
    // TC-47: read the shared, replicated `campaigns` anchor so every agent sees
    // the same set.
    let mut rows: Vec<(ActionHashB64, FundingCampaign)> = Vec::new();

    for link in links_from(&campaigns_anchor()?, LinkTypes::CampaignCatalog)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        let Ok(campaign) = FundingCampaign::try_from(&record) else {
            continue;
        };

        if let Some(state) = &input.state {
            if campaign.state.as_str() != state {
                continue;
            }
        }
        if let Some(enterprise_id) = &input.enterprise_id {
            if &campaign.enterprise_id != enterprise_id {
                continue;
            }
        }

        rows.push((ActionHashB64::from(action_hash), campaign));
    }

    // Newest `created_at` first.
    rows.sort_by(|a, b| {
        b.1.created_at
            .cmp(&a.1.created_at)
            .then_with(|| b.1.id.cmp(&a.1.id))
    });

    let limit = input.limit.unwrap_or(100).max(0) as usize;
    rows.truncate(limit);
    Ok(rows)
}

#[hdk_extern]
fn get_campaign(address: String) -> ExternResult<Option<(ActionHashB64, FundingCampaign)>> {
    let action_hash = ActionHashB64::from_str(&address)
        .map_err(|_| wasm_error!(WasmErrorInner::Guest(format!("invalid campaign address: {address}"))))?
        .into();

    let Some(record) = latest_record(&action_hash)? else {
        return Ok(None);
    };
    let Ok(campaign) = FundingCampaign::try_from(&record) else {
        return Ok(None);
    };
    Ok(Some((ActionHashB64::from(action_hash), campaign)))
}

// ---------------------------------------------------------------------------
// Contributions
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct PledgeInput {
    pub campaign_id: String,
    pub product_variant_id: String,
    pub quantity: i64,
    pub unit_price: String,
    pub plan: ContributionPlan,
}

#[hdk_extern]
fn pledge_to_campaign(input: PledgeInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    if input.quantity <= 0 {
        return guest_error("quantity must be > 0");
    }
    if input.unit_price.is_empty() {
        return guest_error("unit_price must not be empty");
    }
    rules::validate_contribution(
        &input.campaign_id,
        input.quantity,
        &input.unit_price,
        "0.00",
    )
    .map_err(|message| wasm_error!(message))?;

    // TC-20: exact decimal arithmetic, no binary floats.
    let unit_price = Decimal::parse(&input.unit_price)
        .map_err(|message| wasm_error!(message))?;
    let total_amount = unit_price.mul_i64(input.quantity).to_fixed(2);

    // Both plans start unpaid; Full is a single installment, Installments three.
    // Payments are recorded explicitly via `record_payment`.
    let (installments_total, installments_paid) = match input.plan {
        ContributionPlan::Full => (1, 0),
        ContributionPlan::Installments => (3, 0),
    };
    let paid_amount = "0".to_string();

    let campaign_owner_id = resolve_campaign_owner(&input.campaign_id)?
        .unwrap_or_else(|| agent_b64(&author));

    let id = new_id()?;
    let contribution = FundingContribution {
        id: id.clone(),
        campaign_id: input.campaign_id.clone(),
        contributor_id: agent_b64(&author),
        quantity: input.quantity,
        unit_price: input.unit_price,
        total_amount: total_amount.clone(),
        plan: input.plan,
        paid_amount,
        installments_total,
        installments_paid,
        next_due_at: None,
        created_at: now,
        status: ContributionStatus::Pending,
        campaign_owner_id,
    };

    let action_hash = create_entry(EntryTypes::FundingContribution(contribution))?;

    // campaign/{campaign_id} -> contribution (type 1)
    create_link(
        campaign_anchor(&input.campaign_id)?,
        action_hash.clone(),
        LinkTypes::CampaignContribution,
        LinkTag::new(Vec::new()),
    )?;
    // profile/{contributor} -> contribution (type 5)
    create_link(
        profile_anchor(&author)?,
        action_hash.clone(),
        LinkTypes::ProfileContribution,
        LinkTag::new(Vec::new()),
    )?;

    emit_signal(Signal::ContributionMade {
        campaign_id: input.campaign_id,
        contributor_id: AgentPubKeyB64::from(author).to_string(),
        amount: total_amount,
    })?;

    Ok(action_hash.into())
}

#[hdk_extern]
fn get_my_contributions() -> ExternResult<Vec<(ActionHashB64, FundingContribution)>> {
    let author = caller()?;
    let mut rows = Vec::new();

    for link in links_from(&profile_anchor(&author)?, LinkTypes::ProfileContribution)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        if let Ok(contribution) = FundingContribution::try_from(&record) {
            rows.push((ActionHashB64::from(action_hash), contribution));
        }
    }

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Contribution lifecycle (accept / decline / pay / default)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct ContributionDecisionInput {
    pub campaign_id: String,
    pub contribution_id: String,
}

fn contribution_not_found() -> WasmError {
    wasm_error!(WasmErrorInner::Guest("contribution not found".to_string()))
}

/// Campaign owner accepts a pending pledge (Pending -> Accepted).
#[hdk_extern]
fn accept_contribution(input: ContributionDecisionInput) -> ExternResult<()> {
    let author = caller()?;
    let (hash, contribution) = find_contribution(&input.campaign_id, &input.contribution_id)?
        .ok_or_else(contribution_not_found)?;

    if contribution.campaign_owner_id != agent_b64(&author) {
        return guest_error("only the campaign owner may accept a contribution");
    }

    let to = ContributionStatus::Accepted;
    if !transitions::contribution_allowed(contribution.status.as_str(), to.as_str()) {
        return guest_error(format!(
            "invalid contribution transition {} -> {}",
            contribution.status.as_str(),
            to.as_str()
        ));
    }

    let updated = FundingContribution {
        status: to,
        ..contribution
    };
    update_entry(hash, EntryTypes::FundingContribution(updated))?;
    Ok(())
}

/// Campaign owner declines, or contributor withdraws, a pending pledge
/// (Pending -> Declined).
#[hdk_extern]
fn decline_contribution(input: ContributionDecisionInput) -> ExternResult<()> {
    let author = caller()?;
    let (hash, contribution) = find_contribution(&input.campaign_id, &input.contribution_id)?
        .ok_or_else(contribution_not_found)?;

    let author_b64 = agent_b64(&author);
    let is_owner = contribution.campaign_owner_id == author_b64;
    let is_contributor = contribution.contributor_id == author_b64;
    if !is_owner && !is_contributor {
        return guest_error("only the campaign owner or the contributor may decline a contribution");
    }

    let to = ContributionStatus::Declined;
    if !transitions::contribution_allowed(contribution.status.as_str(), to.as_str()) {
        return guest_error(format!(
            "invalid contribution transition {} -> {}",
            contribution.status.as_str(),
            to.as_str()
        ));
    }

    let updated = FundingContribution {
        status: to,
        ..contribution
    };
    update_entry(hash, EntryTypes::FundingContribution(updated))?;
    Ok(())
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct RecordPaymentInput {
    pub campaign_id: String,
    pub contribution_id: String,
    pub amount: String,
}

/// Record a payment against a contribution (Accepted -> Completed when fully paid).
#[hdk_extern]
fn record_payment(input: RecordPaymentInput) -> ExternResult<()> {
    let author = caller()?;
    rules::validate_payment(&input.amount).map_err(|message| wasm_error!(message))?;

    let (hash, contribution) = find_contribution(&input.campaign_id, &input.contribution_id)?
        .ok_or_else(contribution_not_found)?;

    let author_b64 = agent_b64(&author);
    let is_contributor = contribution.contributor_id == author_b64;
    let is_owner = contribution.campaign_owner_id == author_b64;
    if !is_contributor && !is_owner {
        return guest_error("only the contributor or campaign owner may record a payment");
    }

    let payment = Decimal::parse(&input.amount).map_err(|message| wasm_error!(message))?;
    let paid = Decimal::parse(&contribution.paid_amount).map_err(|message| wasm_error!(message))?;
    let total = Decimal::parse(&contribution.total_amount).map_err(|message| wasm_error!(message))?;

    let new_paid = paid.add(&payment).to_fixed(2);
    let new_paid_dec = Decimal::parse(&new_paid).map_err(|message| wasm_error!(message))?;
    if new_paid_dec.cmp_value(&total) == Ordering::Greater {
        return guest_error("payment exceeds the outstanding amount");
    }

    let fully_paid = new_paid_dec.cmp_value(&total) == Ordering::Equal;
    let status = if fully_paid {
        ContributionStatus::Completed
    } else {
        contribution.status
    };
    if fully_paid && !transitions::contribution_allowed(contribution.status.as_str(), status.as_str())
    {
        return guest_error(format!(
            "cannot complete a contribution in state {}",
            contribution.status.as_str()
        ));
    }

    let installments_paid = if contribution.installments_total > 0 && total.mantissa != 0 {
        ((new_paid_dec.mantissa * contribution.installments_total as i128) / total.mantissa) as i64
    } else {
        contribution.installments_paid
    };

    let updated = FundingContribution {
        paid_amount: new_paid,
        installments_paid,
        status,
        ..contribution
    };
    update_entry(hash, EntryTypes::FundingContribution(updated))?;
    Ok(())
}

/// Mark a contribution in default (failure to pay). Contributor or owner.
#[hdk_extern]
fn default_contribution(input: ContributionDecisionInput) -> ExternResult<()> {
    let author = caller()?;
    let (hash, contribution) = find_contribution(&input.campaign_id, &input.contribution_id)?
        .ok_or_else(contribution_not_found)?;

    let author_b64 = agent_b64(&author);
    let is_contributor = contribution.contributor_id == author_b64;
    let is_owner = contribution.campaign_owner_id == author_b64;
    if !is_contributor && !is_owner {
        return guest_error("only the contributor or campaign owner may mark a default");
    }

    let to = ContributionStatus::Defaulted;
    if !transitions::contribution_allowed(contribution.status.as_str(), to.as_str()) {
        return guest_error(format!(
            "invalid contribution transition {} -> {}",
            contribution.status.as_str(),
            to.as_str()
        ));
    }

    let updated = FundingContribution {
        status: to,
        ..contribution
    };
    update_entry(hash, EntryTypes::FundingContribution(updated))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Batches and reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct CreateBatchInput {
    pub campaign_id: String,
    pub name: String,
    pub crop: String,
    pub variety: Option<String>,
    pub target_quantity: Option<String>,
    pub unit: Option<String>,
    pub sowing_date: Option<i64>,
    pub expected_harvest_at: Option<i64>,
}

#[hdk_extern]
fn create_batch(input: CreateBatchInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_batch(&input.name, &input.campaign_id, &input.crop)
        .map_err(|message| wasm_error!(message))?;

    let owner_id = resolve_campaign_owner(&input.campaign_id)?.unwrap_or_else(|| agent_b64(&author));
    let id = new_id()?;
    let batch = FundingBatch {
        id: id.clone(),
        campaign_id: input.campaign_id,
        owner_id,
        name: input.name,
        crop: input.crop,
        variety: input.variety,
        target_quantity: input.target_quantity,
        unit: input.unit,
        sowing_date: input.sowing_date,
        expected_harvest_at: input.expected_harvest_at,
        status: BatchStatus::Planned,
        created_at: now,
        updated_at: now,
    };

    let action_hash = create_entry(EntryTypes::FundingBatch(batch.clone()))?;
    create_link(
        campaign_anchor(&batch.campaign_id)?,
        action_hash.clone(),
        LinkTypes::CampaignBatch,
        LinkTag::new(Vec::new()),
    )?;

    Ok(action_hash.into())
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct PostBatchReportInput {
    pub batch_id: String,
    pub stage: String,
    pub health: String,
    pub expected_yield: Option<String>,
    pub actual_yield: Option<String>,
    pub notes: Option<String>,
}

#[hdk_extern]
fn post_batch_report(input: PostBatchReportInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_batch_report(&input.batch_id, &input.stage, &input.health)
        .map_err(|message| wasm_error!(message))?;

    let id = new_id()?;
    let report = BatchReport {
        id: id.clone(),
        batch_id: input.batch_id.clone(),
        report_date: now,
        stage: input.stage,
        health: input.health,
        expected_yield: input.expected_yield,
        actual_yield: input.actual_yield,
        notes: input.notes,
        created_by: agent_b64(&author),
        created_at: now,
    };

    let action_hash = create_entry(EntryTypes::BatchReport(report))?;
    create_link(
        batch_anchor(&input.batch_id)?,
        action_hash.clone(),
        LinkTypes::BatchReport,
        LinkTag::new(Vec::new()),
    )?;

    emit_signal(Signal::BatchReportPosted {
        batch_id: input.batch_id,
        report_id: id,
    })?;

    Ok(action_hash.into())
}

#[hdk_extern]
fn get_batch_reports(batch_id: String) -> ExternResult<Vec<(ActionHashB64, BatchReport)>> {
    let mut rows = Vec::new();
    for link in links_from(&batch_anchor(&batch_id)?, LinkTypes::BatchReport)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        if let Ok(report) = BatchReport::try_from(&record) {
            rows.push((ActionHashB64::from(action_hash), report));
        }
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Campaign state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct SetCampaignStateInput {
    pub campaign_id: String,
    pub state: FundingState,
}

#[hdk_extern]
fn set_campaign_state(input: SetCampaignStateInput) -> ExternResult<()> {
    let author = caller()?;
    let now = now_secs()?;

    let (latest_action_hash, campaign) = find_campaign(&input.campaign_id)?.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "campaign {} not found",
            input.campaign_id
        )))
    })?;

    if campaign.owner_id != agent_b64(&author) {
        return guest_error("only the campaign owner may change its state");
    }

    let from = campaign.state.as_str();
    let to = input.state.as_str();
    if !transitions::campaign_allowed(from, to) {
        return guest_error(format!("invalid campaign transition {from} -> {to}"));
    }

    let updated = FundingCampaign {
        state: input.state,
        updated_at: now,
        ..campaign
    };
    update_entry(latest_action_hash, EntryTypes::FundingCampaign(updated))?;

    emit_signal(Signal::CampaignStateChanged {
        campaign_id: input.campaign_id,
        state: to.to_string(),
    })?;

    Ok(())
}

/// Recompute a campaign's `current_amount` and `fulfilled_quantity` from its
/// contributions. Owner-only. Declined and Defaulted contributions are excluded.
#[hdk_extern]
fn recalculate_campaign_totals(campaign_id: String) -> ExternResult<()> {
    let author = caller()?;
    let now = now_secs()?;

    let (latest_action_hash, campaign) = find_campaign(&campaign_id)?.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "campaign {} not found",
            campaign_id
        )))
    })?;

    if campaign.owner_id != agent_b64(&author) {
        return guest_error("only the campaign owner may recalculate totals");
    }

    let mut total = Decimal::zero();
    let mut fulfilled: i64 = 0;

    for link in links_from(&campaign_anchor(&campaign_id)?, LinkTypes::CampaignContribution)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        let Ok(contribution) = FundingContribution::try_from(&record) else {
            continue;
        };
        if matches!(contribution.status.as_str(), "Declined" | "Defaulted") {
            continue;
        }
        let amount = Decimal::parse(&contribution.total_amount)
            .map_err(|message| wasm_error!(message))?;
        total = total.add(&amount);
        fulfilled += contribution.quantity;
    }

    let updated = FundingCampaign {
        current_amount: total.to_fixed(2),
        fulfilled_quantity: fulfilled,
        updated_at: now,
        ..campaign
    };
    update_entry(latest_action_hash, EntryTypes::FundingCampaign(updated))?;

    Ok(())
}

/// Extra (not in the required surface): expose the batch state machine so the
/// §4.2 table can be exercised end-to-end. Documented in README.
#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct SetBatchStatusInput {
    pub campaign_id: String,
    pub batch_id: String,
    pub status: BatchStatus,
}

#[hdk_extern]
fn set_batch_status(input: SetBatchStatusInput) -> ExternResult<()> {
    let author = caller()?;
    let now = now_secs()?;

    // Batches are linked from the campaign anchor (type 2), so resolve via it.
    let base = campaign_anchor(&input.campaign_id)?;
    let mut found: Option<(ActionHash, FundingBatch)> = None;
    for link in links_from(&base, LinkTypes::CampaignBatch)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        if let Some(record) = latest_record(&action_hash)? {
            if let Ok(batch) = FundingBatch::try_from(&record) {
                // `create_batch` returns the create action hash, while the entry
                // also carries a 32-hex id; accept either form.
                let matches = batch.id == input.batch_id
                    || ActionHashB64::from(action_hash.clone()).to_string() == input.batch_id;
                if matches {
                    found = Some((record.action_address().clone(), batch));
                    break;
                }
            }
        }
    }

    let (latest_action_hash, batch) = found.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "batch {} not found",
            input.batch_id
        )))
    })?;

    if batch.owner_id != agent_b64(&author) {
        return guest_error("only the batch owner may change its status");
    }

    let from = batch.status.as_str();
    let to = input.status.as_str();
    if !transitions::batch_allowed(from, to) {
        return guest_error(format!("invalid batch transition {from} -> {to}"));
    }

    let updated = FundingBatch {
        status: input.status,
        updated_at: now,
        ..batch
    };
    update_entry(latest_action_hash, EntryTypes::FundingBatch(updated))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Projections
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct ProjectionLineInput {
    pub period_index: i64,
    pub category: String,
    pub description: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub unit_cost: Option<String>,
    pub amount: String,
    pub sort_order: i64,
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
pub struct CreateProjectionInput {
    pub campaign_id: String,
    pub periods: i64,
    pub start_month: i64,
    pub target_margin_pct: String,
    pub notes: Option<String>,
    pub lines: Vec<ProjectionLineInput>,
}

#[hdk_extern]
fn create_projection(input: CreateProjectionInput) -> ExternResult<ActionHashB64> {
    let author = caller()?;
    let now = now_secs()?;

    rules::validate_projection(&input.campaign_id, input.periods)
        .map_err(|message| wasm_error!(message))?;

    let owner_id = resolve_campaign_owner(&input.campaign_id)?.unwrap_or_else(|| agent_b64(&author));

    let id = new_id()?;
    let projection = FundingProjection {
        id: id.clone(),
        campaign_id: input.campaign_id.clone(),
        owner_id,
        periods: input.periods,
        start_month: input.start_month,
        target_margin_pct: input.target_margin_pct,
        notes: input.notes,
        created_at: now,
        updated_at: now,
    };

    let projection_hash = create_entry(EntryTypes::FundingProjection(projection))?;

    // campaign/{campaign_id} -> projection (type 4)   [TC-52]
    create_link(
        campaign_anchor(&input.campaign_id)?,
        projection_hash.clone(),
        LinkTypes::CampaignProjection,
        LinkTag::new(Vec::new()),
    )?;

    for (index, line) in input.lines.into_iter().enumerate() {
        if line.amount.is_empty() {
            return guest_error("line amount must not be empty");
        }
        rules::validate_projection_line(&id, &line.amount)
            .map_err(|message| wasm_error!(message))?;

        let line_entry = ProjectionLine {
            id: new_id()?,
            // The parent projection's action address lets the integrity layer
            // authorise lines against the parent owner (TC-43).
            projection_id: ActionHashB64::from(projection_hash.clone()).to_string(),
            period_index: line.period_index,
            category: line.category,
            description: line.description,
            quantity: line.quantity,
            unit: line.unit,
            unit_cost: line.unit_cost,
            amount: line.amount,
            sort_order: line.sort_order,
            created_at: now,
        };

        let line_hash = create_entry(EntryTypes::ProjectionLine(line_entry))?;

        // campaign/{campaign_id} -> line (type 4); validation accepts lines.
        create_link(
            campaign_anchor(&input.campaign_id)?,
            line_hash,
            LinkTypes::CampaignProjection,
            LinkTag::new(Vec::new()),
        )?;

        let _ = index;
    }

    Ok(projection_hash.into())
}

#[hdk_extern]
fn get_projection(
    campaign_id: String,
) -> ExternResult<Option<(ActionHashB64, FundingProjection)>> {
    for link in links_from(&campaign_anchor(&campaign_id)?, LinkTypes::CampaignProjection)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        if let Ok(projection) = FundingProjection::try_from(&record) {
            if projection.campaign_id == campaign_id {
                return Ok(Some((ActionHashB64::from(action_hash), projection)));
            }
        }
    }
    Ok(None)
}

/// Extra read (not in the required surface): projection lines, sorted by
/// `period_index` then `sort_order`.
#[hdk_extern]
fn get_projection_lines(
    campaign_id: String,
) -> ExternResult<Vec<(ActionHashB64, ProjectionLine)>> {
    let mut rows: Vec<(ActionHashB64, ProjectionLine)> = Vec::new();
    for link in links_from(&campaign_anchor(&campaign_id)?, LinkTypes::CampaignProjection)? {
        let Some(action_hash) = action_hash_of(&link.target) else {
            continue;
        };
        let Some(record) = latest_record(&action_hash)? else {
            continue;
        };
        if let Ok(line) = ProjectionLine::try_from(&record) {
            rows.push((ActionHashB64::from(action_hash), line));
        }
    }
    rows.sort_by(|a, b| {
        a.1.period_index
            .cmp(&b.1.period_index)
            .then_with(|| a.1.sort_order.cmp(&b.1.sort_order))
    });
    Ok(rows)
}