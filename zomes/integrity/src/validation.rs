//! Integrity validation: entry rules, authorisation, state transitions, link checks.
//!
//! Implements SPEC §4 (state machines), §6 (authorisation), §12 (required
//! validation) and INTERFACE_CONTRACT §7, §10, §12.

use std::cmp::Ordering;
use std::str::FromStr;

use hdi::prelude::*;
use harvest_advance_logic::{rules, transitions, Decimal};
use holo_hash::{ActionHashB64, AgentPubKeyB64, AnyLinkableHashPrimitive};

use crate::entries::*;
use crate::links::LinkTypes;

fn agent_b64(key: &AgentPubKey) -> String {
    AgentPubKeyB64::from(key.clone()).to_string()
}

fn invalid(message: impl Into<String>) -> ValidateCallbackResult {
    ValidateCallbackResult::Invalid(message.into())
}

/// The integrity `validate` callback.
#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    let flat = op.flattened::<EntryTypes, LinkTypes>()?;

    match flat {
        FlatOp::CreateEntry(OpEntry::CreateEntry { app_entry, action }) => {
            Ok(validate_create_entry(&app_entry, action.author()))
        }
        FlatOp::CreateEntry(OpEntry::UpdateEntry { app_entry, action }) => {
            Ok(validate_update_entry(
                &app_entry,
                action.author(),
                &action.data.original_entry_address,
            ))
        }
        FlatOp::CreateRecord(OpRecord::CreateEntry { app_entry, action }) => {
            Ok(validate_create_entry(&app_entry, action.author()))
        }
        FlatOp::CreateRecord(OpRecord::UpdateEntry { app_entry, action }) => {
            Ok(validate_update_entry(
                &app_entry,
                action.author(),
                &action.data.original_entry_address,
            ))
        }
        FlatOp::Update(OpUpdate::Entry { app_entry, action }) => Ok(validate_update_entry(
            &app_entry,
            action.author(),
            &action.data.original_entry_address,
        )),
        FlatOp::Link(OpLink::CreateLink { link_type, action }) => Ok(validate_create_link(
            link_type,
            &action.data.base_address,
            &action.data.target_address,
            action.author(),
        )),
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

/// Required-field, numeric and enum checks plus create-time authorisation.
fn validate_create_entry(entry: &EntryTypes, author: &AgentPubKey) -> ValidateCallbackResult {
    match entry {
        EntryTypes::Enterprise(e) => {
            if let Err(message) = rules::validate_enterprise(&e.name, &e.permalink) {
                return invalid(message);
            }
            if e.owner_id != agent_b64(author) {
                return invalid("Enterprise.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FundingCampaign(c) => {
            if let Err(message) = rules::validate_campaign(
                &c.name,
                &c.enterprise_id,
                &c.goal_amount,
                c.target_quantity,
                c.start_at,
                c.end_at,
            ) {
                return invalid(message);
            }
            if c.owner_id != agent_b64(author) {
                return invalid("FundingCampaign.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FundingContribution(c) => {
            if let Err(message) =
                rules::validate_contribution(&c.campaign_id, c.quantity, &c.unit_price, &c.total_amount)
            {
                return invalid(message);
            }
            if c.contributor_id != agent_b64(author) {
                return invalid("FundingContribution.contributor_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FundingBatch(b) => {
            if let Err(message) = rules::validate_batch(&b.name, &b.campaign_id, &b.crop) {
                return invalid(message);
            }
            if b.owner_id != agent_b64(author) {
                return invalid("FundingBatch.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::BatchReport(r) => {
            if let Err(message) = rules::validate_batch_report(&r.batch_id, &r.stage, &r.health) {
                return invalid(message);
            }
            if r.created_by != agent_b64(author) {
                return invalid("BatchReport.created_by must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FundingProjection(p) => {
            if let Err(message) = rules::validate_projection(&p.campaign_id, p.periods) {
                return invalid(message);
            }
            if p.owner_id != agent_b64(author) {
                return invalid("FundingProjection.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::ProjectionLine(line) => {
            if let Err(message) =
                rules::validate_projection_line(&line.projection_id, &line.amount)
            {
                return invalid(message);
            }
            match resolve_projection_owner(&line.projection_id) {
                Some(owner) if owner == agent_b64(author) => ValidateCallbackResult::Valid,
                Some(_) => invalid("ProjectionLine author must match the parent projection owner"),
                None => invalid("ProjectionLine parent projection could not be resolved"),
            }
        }
        EntryTypes::PickupLocation(l) => {
            if let Err(message) = rules::validate_pickup_location(&l.name, &l.enterprise_id) {
                return invalid(message);
            }
            if l.owner_id != agent_b64(author) {
                return invalid("PickupLocation.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::Profile(p) => {
            if let Err(message) = rules::validate_profile(&p.email) {
                return invalid(message);
            }
            if p.id != agent_b64(author) {
                return invalid("Profile.id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FarmersCluster(c) => {
            if let Err(message) = rules::validate_farmers_cluster(&c.name, &c.region) {
                return invalid(message);
            }
            if c.owner_id != agent_b64(author) {
                return invalid("FarmersCluster.owner_id must equal the author");
            }
            ValidateCallbackResult::Valid
        }
    }
}

/// Updates must re-satisfy every create-time check (including re-asserting the
/// owner field, SPEC §6.2) and must obey the state machines (SPEC §4).
fn validate_update_entry(
    entry: &EntryTypes,
    author: &AgentPubKey,
    original_entry_hash: &EntryHash,
) -> ValidateCallbackResult {
    // Contributions have their own authorisation model (contributor OR owner),
    // so they bypass the strict create check (which requires contributor == author).
    if let EntryTypes::FundingContribution(new) = entry {
        return validate_update_contribution(new, author, original_entry_hash);
    }

    let create_result = validate_create_entry(entry, author);
    if !matches!(create_result, ValidateCallbackResult::Valid) {
        return create_result;
    }

    match entry {
        EntryTypes::FundingCampaign(new) => {
            if let Some(old) = read_previous_campaign(original_entry_hash) {
                // Only enforce the state machine when the state actually changes;
                // non-state updates (e.g. recalculating totals) keep the same state.
                if old.state != new.state
                    && !transitions::campaign_allowed(old.state.as_str(), new.state.as_str())
                {
                    return invalid(format!(
                        "invalid campaign transition {} -> {}",
                        old.state.as_str(),
                        new.state.as_str()
                    ));
                }
            }
            ValidateCallbackResult::Valid
        }
        EntryTypes::FundingBatch(new) => {
            match read_previous_batch(original_entry_hash) {
                Some(old) => {
                    if !transitions::batch_allowed(old.status.as_str(), new.status.as_str()) {
                        return invalid(format!(
                            "invalid batch transition {} -> {}",
                            old.status.as_str(),
                            new.status.as_str()
                        ));
                    }
                }
                None => {}
            }
            ValidateCallbackResult::Valid
        }
        _ => ValidateCallbackResult::Valid,
    }
}

/// Validate a contribution update (accept / decline / payment / default).
///
/// The author may be the contributor or the campaign owner. Immutable fields
/// must be preserved, the status transition must be legal, and money fields must
/// move monotonically without exceeding the total.
fn validate_update_contribution(
    new: &FundingContribution,
    author: &AgentPubKey,
    original_entry_hash: &EntryHash,
) -> ValidateCallbackResult {
    if let Err(message) = rules::validate_contribution(
        &new.campaign_id,
        new.quantity,
        &new.unit_price,
        &new.total_amount,
    ) {
        return invalid(message);
    }

    let Some(old) = read_previous_contribution(original_entry_hash) else {
        // Fail open when the previous record cannot be read (matches campaign/batch).
        return ValidateCallbackResult::Valid;
    };

    if old.contributor_id != new.contributor_id
        || old.campaign_id != new.campaign_id
        || old.campaign_owner_id != new.campaign_owner_id
        || old.quantity != new.quantity
        || old.unit_price != new.unit_price
        || old.total_amount != new.total_amount
        || old.plan != new.plan
    {
        return invalid("contribution immutable fields must not change");
    }

    let author_b64 = agent_b64(author);
    let is_contributor = new.contributor_id == author_b64;
    let is_owner = new.campaign_owner_id == author_b64;
    if !is_contributor && !is_owner {
        return invalid("only the contributor or campaign owner may update a contribution");
    }

    // Only enforce the state machine when the status actually changes; a payment
    // that leaves the status unchanged (e.g. partial payment) is a valid update.
    if old.status != new.status
        && !transitions::contribution_allowed(old.status.as_str(), new.status.as_str())
    {
        return invalid(format!(
            "invalid contribution transition {} -> {}",
            old.status.as_str(),
            new.status.as_str()
        ));
    }

    // Money: paid_amount must be a valid, non-decreasing decimal not exceeding total.
    let old_paid = Decimal::parse(&old.paid_amount).ok();
    let new_paid = Decimal::parse(&new.paid_amount).ok();
    let total = Decimal::parse(&new.total_amount).ok();
    if let (Some(o), Some(n)) = (old_paid.as_ref(), new_paid.as_ref()) {
        if n.cmp_value(o) == Ordering::Less {
            return invalid("paid_amount must not decrease");
        }
    }
    if let (Some(n), Some(t)) = (new_paid.as_ref(), total.as_ref()) {
        if n.cmp_value(t) == Ordering::Greater {
            return invalid("paid_amount must not exceed total_amount");
        }
    }

    ValidateCallbackResult::Valid
}

fn read_previous_contribution(hash: &EntryHash) -> Option<FundingContribution> {
    let entry = must_get_entry(hash.clone()).ok()?;
    FundingContribution::try_from(entry).ok()
}

fn read_previous_campaign(hash: &EntryHash) -> Option<FundingCampaign> {
    let entry = must_get_entry(hash.clone()).ok()?;
    FundingCampaign::try_from(entry).ok()
}

fn read_previous_batch(hash: &EntryHash) -> Option<FundingBatch> {
    let entry = must_get_entry(hash.clone()).ok()?;
    FundingBatch::try_from(entry).ok()
}

/// Resolve the owner of the projection whose action address is `projection_id`.
fn resolve_projection_owner(projection_id: &str) -> Option<String> {
    let action_hash = ActionHashB64::from_str(projection_id).ok()?.into();
    let action = must_get_action(action_hash).ok()?;
    let entry_hash = action.hashed.content.entry_hash().cloned()?;
    let entry = must_get_entry(entry_hash).ok()?;
    FundingProjection::try_from(entry).ok().map(|p| p.owner_id)
}

/// Per-link-type validation (SPEC §6.3; TC-45, TC-46).
fn validate_create_link(
    link_type: LinkTypes,
    base: &AnyLinkableHash,
    target: &AnyLinkableHash,
    author: &AgentPubKey,
) -> ValidateCallbackResult {
    if base == target {
        return invalid("self-links (base == target) are not allowed");
    }

    let base_hash = match base.clone().into_primitive() {
        AnyLinkableHashPrimitive::Entry(hash) => hash,
        _ => return invalid("link base must be an anchor entry hash"),
    };

    let entry = match resolve_target_entry(target) {
        Some(entry) => entry,
        None => return invalid("link target entry could not be resolved"),
    };

    match link_type {
        LinkTypes::EnterpriseCampaign => match entry {
            EntryTypes::FundingCampaign(c) => {
                if let Err(result) = check_anchor(
                    &base_hash,
                    &[
                        format!("enterprise/{}", c.enterprise_id),
                        format!("campaign/{}", c.id),
                    ],
                ) {
                    return result;
                }
                if c.owner_id != agent_b64(author) {
                    return invalid("EnterpriseCampaign author must own the campaign");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("EnterpriseCampaign must target a FundingCampaign"),
        },
        LinkTypes::CampaignContribution => match entry {
            EntryTypes::FundingContribution(c) => {
                if let Err(result) =
                    check_anchor(&base_hash, &[format!("campaign/{}", c.campaign_id)])
                {
                    return result;
                }
                if c.contributor_id != agent_b64(author) {
                    return invalid("CampaignContribution author must be the contributor");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("CampaignContribution must target a FundingContribution"),
        },
        LinkTypes::CampaignBatch => match entry {
            EntryTypes::FundingBatch(b) => {
                if let Err(result) =
                    check_anchor(&base_hash, &[format!("campaign/{}", b.campaign_id)])
                {
                    return result;
                }
                if b.owner_id != agent_b64(author) {
                    return invalid("CampaignBatch author must own the batch");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("CampaignBatch must target a FundingBatch"),
        },
        LinkTypes::BatchReport => match entry {
            EntryTypes::BatchReport(r) => {
                if let Err(result) = check_anchor(&base_hash, &[format!("batch/{}", r.batch_id)]) {
                    return result;
                }
                if r.created_by != agent_b64(author) {
                    return invalid("BatchReport author must be the report creator");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("BatchReport link must target a BatchReport"),
        },
        LinkTypes::CampaignProjection => match entry {
            EntryTypes::FundingProjection(p) => {
                if let Err(result) =
                    check_anchor(&base_hash, &[format!("campaign/{}", p.campaign_id)])
                {
                    return result;
                }
                if p.owner_id != agent_b64(author) {
                    return invalid("CampaignProjection author must own the projection");
                }
                ValidateCallbackResult::Valid
            }
            EntryTypes::ProjectionLine(line) => {
                // Lines are linked from the same campaign anchor; authorise
                // against the parent projection owner (SPEC §6.2, TC-43).
                match resolve_projection(&line.projection_id) {
                    Some(parent) => {
                        if let Err(result) =
                            check_anchor(&base_hash, &[format!("campaign/{}", parent.campaign_id)])
                        {
                            return result;
                        }
                        if parent.owner_id != agent_b64(author) {
                            return invalid(
                                "CampaignProjection author must own the parent projection",
                            );
                        }
                        ValidateCallbackResult::Valid
                    }
                    None => invalid("CampaignProjection line parent could not be resolved"),
                }
            }
            _ => invalid("CampaignProjection must target a FundingProjection or ProjectionLine"),
        },
        LinkTypes::ProfileContribution => match entry {
            EntryTypes::FundingContribution(c) => {
                if let Err(result) =
                    check_anchor(&base_hash, &[format!("profile/{}", c.contributor_id)])
                {
                    return result;
                }
                if c.contributor_id != agent_b64(author) {
                    return invalid("ProfileContribution author must be the contributor");
                }
                ValidateCallbackResult::Valid
            }
            // Profiles are also indexed under the profile anchor (used to enforce
            // one-profile-per-agent, TC-29).
            EntryTypes::Profile(p) => {
                if let Err(result) = check_anchor(&base_hash, &[format!("profile/{}", p.id)]) {
                    return result;
                }
                if p.id != agent_b64(author) {
                    return invalid("ProfileContribution author must own the profile");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("ProfileContribution must target a FundingContribution or Profile"),
        },
        LinkTypes::EnterpriseLocation => match entry {
            EntryTypes::PickupLocation(l) => {
                if let Err(result) =
                    check_anchor(&base_hash, &[format!("enterprise/{}", l.enterprise_id)])
                {
                    return result;
                }
                if l.owner_id != agent_b64(author) {
                    return invalid("EnterpriseLocation author must own the location");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("EnterpriseLocation must target a PickupLocation"),
        },
        // Reserved: no operation creates this link. Accept so that a future
        // catalogue integration is not blocked.
        LinkTypes::CampaignProduct => ValidateCallbackResult::Valid,
        LinkTypes::EnterpriseProfile => match entry {
            EntryTypes::Enterprise(e) => {
                if let Err(result) = check_anchor(&base_hash, &[format!("enterprise/{}", e.id)]) {
                    return result;
                }
                if e.owner_id != agent_b64(author) {
                    return invalid("EnterpriseProfile author must own the enterprise");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("EnterpriseProfile must target an Enterprise"),
        },
        LinkTypes::ClusterList => match entry {
            EntryTypes::FarmersCluster(c) => {
                if let Err(result) = check_anchor(&base_hash, &["clusters".to_string()]) {
                    return result;
                }
                if c.owner_id != agent_b64(author) {
                    return invalid("ClusterList author must own the cluster");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("ClusterList must target a FarmersCluster"),
        },
        LinkTypes::CampaignCatalog => match entry {
            EntryTypes::FundingCampaign(c) => {
                if let Err(result) = check_anchor(&base_hash, &["campaigns".to_string()]) {
                    return result;
                }
                if c.owner_id != agent_b64(author) {
                    return invalid("CampaignCatalog author must own the campaign");
                }
                ValidateCallbackResult::Valid
            }
            _ => invalid("CampaignCatalog must target a FundingCampaign"),
        },
    }
}

/// Verify the link base equals the hash of one of the allowed anchor paths.
fn check_anchor(
    base: &EntryHash,
    allowed_paths: &[String],
) -> Result<(), ValidateCallbackResult> {
    for path in allowed_paths {
        match Path::from(path.as_str()).path_entry_hash() {
            Ok(expected) if expected == *base => return Ok(()),
            Ok(_) => {}
            Err(_) => return Err(invalid("could not compute anchor hash")),
        }
    }
    Err(invalid(format!(
        "link base is not one of the expected anchors: {}",
        allowed_paths.join(", ")
    )))
}

/// Resolve a link target (usually an action hash) to its app entry.
fn resolve_target_entry(target: &AnyLinkableHash) -> Option<EntryTypes> {
    match target.clone().into_primitive() {
        AnyLinkableHashPrimitive::Action(action_hash) => {
            let action = must_get_action(action_hash.clone()).ok()?;
            let entry_hash = action.hashed.content.entry_hash().cloned()?;
            let entry = must_get_entry(entry_hash).ok()?;
            let entry_ref = entry.as_content();
            if let Some(EntryType::App(def)) = action.hashed.content.entry_type() {
                let scoped = ScopedEntryDefIndex {
                    zome_index: def.zome_index,
                    zome_type: def.entry_index,
                };
                if let Ok(unit) = UnitEntryTypes::try_from(scoped) {
                    if let Ok(entry_types) = EntryTypes::try_from((unit, entry_ref)) {
                        return Some(entry_types);
                    }
                }
            }
            try_all_types(entry_ref)
        }
        AnyLinkableHashPrimitive::Entry(entry_hash) => {
            let entry = must_get_entry(entry_hash.clone()).ok()?;
            try_all_types(entry.as_content())
        }
        AnyLinkableHashPrimitive::External(_) => None,
    }
}

fn try_all_types(entry: &Entry) -> Option<EntryTypes> {
    if let Ok(v) = Enterprise::try_from(entry) {
        return Some(EntryTypes::Enterprise(v));
    }
    if let Ok(v) = FundingCampaign::try_from(entry) {
        return Some(EntryTypes::FundingCampaign(v));
    }
    if let Ok(v) = FundingContribution::try_from(entry) {
        return Some(EntryTypes::FundingContribution(v));
    }
    if let Ok(v) = FundingBatch::try_from(entry) {
        return Some(EntryTypes::FundingBatch(v));
    }
    if let Ok(v) = BatchReport::try_from(entry) {
        return Some(EntryTypes::BatchReport(v));
    }
    if let Ok(v) = FundingProjection::try_from(entry) {
        return Some(EntryTypes::FundingProjection(v));
    }
    if let Ok(v) = ProjectionLine::try_from(entry) {
        return Some(EntryTypes::ProjectionLine(v));
    }
    if let Ok(v) = PickupLocation::try_from(entry) {
        return Some(EntryTypes::PickupLocation(v));
    }
    if let Ok(v) = Profile::try_from(entry) {
        return Some(EntryTypes::Profile(v));
    }
    if let Ok(v) = FarmersCluster::try_from(entry) {
        return Some(EntryTypes::FarmersCluster(v));
    }
    None
}

/// Resolve a projection from its action-address string.
fn resolve_projection(projection_id: &str) -> Option<FundingProjection> {
    let action_hash = ActionHashB64::from_str(projection_id).ok()?.into();
    let action = must_get_action(action_hash).ok()?;
    let entry_hash = action.hashed.content.entry_hash().cloned()?;
    let entry = must_get_entry(entry_hash).ok()?;
    FundingProjection::try_from(entry).ok()
}