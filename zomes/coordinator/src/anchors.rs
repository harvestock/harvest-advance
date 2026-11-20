//! Anchor path construction and link reads (SPEC §3.5).

use hdk::prelude::*;
use holo_hash::{ActionHash, AgentPubKeyB64, AnyLinkableHash, AnyLinkableHashPrimitive, EntryHash};

use crate::LinkTypes;

pub fn anchor(path: &str) -> ExternResult<EntryHash> {
    Path::from(path).path_entry_hash()
}

pub fn campaigns_anchor() -> ExternResult<EntryHash> {
    anchor("campaigns")
}

pub fn enterprise_anchor(id: &str) -> ExternResult<EntryHash> {
    anchor(&format!("enterprise/{id}"))
}

pub fn campaign_anchor(id: &str) -> ExternResult<EntryHash> {
    anchor(&format!("campaign/{id}"))
}

pub fn batch_anchor(id: &str) -> ExternResult<EntryHash> {
    anchor(&format!("batch/{id}"))
}

pub fn profile_anchor(key: &AgentPubKey) -> ExternResult<EntryHash> {
    anchor(&format!("profile/{}", AgentPubKeyB64::from(key.clone())))
}

#[allow(dead_code)]
pub fn clusters_anchor() -> ExternResult<EntryHash> {
    anchor("clusters")
}

/// Read all links of `link_type` from `base`.
pub fn links_from(base: &EntryHash, link_type: LinkTypes) -> ExternResult<Vec<Link>> {
    get_links(
        LinkQuery::try_new(base.clone(), link_type)?,
        GetStrategy::default(),
    )
}

/// The action hash a link points at, if it points at a DHT action.
pub fn action_hash_of(target: &AnyLinkableHash) -> Option<ActionHash> {
    match target.clone().into_primitive() {
        AnyLinkableHashPrimitive::Action(hash) => Some(hash),
        _ => None,
    }
}