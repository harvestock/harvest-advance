//! Server-generated identifiers (SPEC §3.1).

use hdk::prelude::*;

/// 128 random bits rendered as 32 lowercase hex characters.
pub fn new_id() -> ExternResult<String> {
    let bytes = random_bytes(16)?;
    let mut array = [0u8; 16];
    array.copy_from_slice(bytes.as_ref());
    Ok(harvest_advance_logic::ids::hex32(&array))
}