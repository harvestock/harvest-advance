//! Pure, host-independent rules for Harvest Advance.
//!
//! This crate deliberately has no dependency on the Holochain crates so that the
//! rules can be unit-tested natively and reused by both the integrity and
//! coordinator zomes.

pub mod decimal;
pub mod ids;
pub mod rules;
pub mod transitions;

pub use decimal::Decimal;