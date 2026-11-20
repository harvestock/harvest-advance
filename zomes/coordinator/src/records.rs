//! CRUD record walking (SPEC §7; TC-32, TC-38).

use hdk::prelude::*;

/// Return the newest revision reachable from `action_hash`, or `None` if the
/// record is missing or deleted.
///
/// The address callers hold is the *original* create action hash; reads walk
/// forward through updates so the address stays stable while the content
/// advances (SPEC §5.6 / §5.7).
pub fn latest_record(action_hash: &ActionHash) -> ExternResult<Option<Record>> {
    let details = match get_details(action_hash.clone(), GetOptions::default())? {
        Some(Details::Record(details)) => details,
        _ => return Ok(None),
    };

    if !details.deletes.is_empty() {
        return Ok(None);
    }

    let mut newest = details.record;
    for update in details.updates {
        let update_hash = update.hashed.hash;
        if let Some(record) = latest_record(&update_hash)? {
            if is_newer(&record, &newest) {
                newest = record;
            }
        }
    }

    Ok(Some(newest))
}

fn is_newer(candidate: &Record, current: &Record) -> bool {
    let candidate_action = candidate.action();
    let current_action = current.action();
    candidate_action.timestamp() > current_action.timestamp()
        || (candidate_action.timestamp() == current_action.timestamp()
            && candidate_action.action_seq() > current_action.action_seq())
}