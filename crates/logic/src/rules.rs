//! Pure field-validation rules (SPEC §3.2, §12; INTERFACE_CONTRACT §12).
//!
//! These functions are the single source of truth for the required-field,
//! numeric and enum checks. The integrity zome calls them; the unit tests call
//! them directly.

use crate::decimal::Decimal;

pub fn non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

pub fn validate_enterprise(name: &str, permalink: &str) -> Result<(), String> {
    non_empty("name", name)?;
    non_empty("permalink", permalink)?;
    Ok(())
}

pub fn validate_campaign(
    name: &str,
    enterprise_id: &str,
    goal_amount: &str,
    target_quantity: i64,
    start_at: i64,
    end_at: i64,
) -> Result<(), String> {
    non_empty("name", name)?;
    non_empty("enterprise_id", enterprise_id)?;

    let goal = Decimal::parse(goal_amount)
        .map_err(|e| format!("goal_amount must be a decimal string: {e}"))?;
    if !goal.is_positive() {
        return Err("goal_amount must be > 0".to_string());
    }

    if target_quantity <= 0 {
        return Err("target_quantity must be > 0".to_string());
    }

    if end_at <= start_at {
        return Err("end_at must be greater than start_at".to_string());
    }

    Ok(())
}

pub fn validate_contribution(
    campaign_id: &str,
    quantity: i64,
    unit_price: &str,
    total_amount: &str,
) -> Result<(), String> {
    non_empty("campaign_id", campaign_id)?;
    if quantity <= 0 {
        return Err("quantity must be > 0".to_string());
    }
    non_empty("unit_price", unit_price)?;
    non_empty("total_amount", total_amount)?;
    Ok(())
}

pub fn validate_payment(amount: &str) -> Result<(), String> {
    let value = Decimal::parse(amount)
        .map_err(|e| format!("amount must be a decimal string: {e}"))?;
    if !value.is_positive() {
        return Err("amount must be > 0".to_string());
    }
    Ok(())
}

pub fn validate_batch(name: &str, campaign_id: &str, crop: &str) -> Result<(), String> {
    non_empty("name", name)?;
    non_empty("campaign_id", campaign_id)?;
    non_empty("crop", crop)?;
    Ok(())
}

pub fn validate_batch_report(batch_id: &str, stage: &str, health: &str) -> Result<(), String> {
    non_empty("batch_id", batch_id)?;
    non_empty("stage", stage)?;
    non_empty("health", health)?;
    Ok(())
}

pub fn validate_projection(campaign_id: &str, periods: i64) -> Result<(), String> {
    non_empty("campaign_id", campaign_id)?;
    if periods < 6 {
        return Err("periods must be >= 6".to_string());
    }
    Ok(())
}

pub fn validate_projection_line(projection_id: &str, amount: &str) -> Result<(), String> {
    non_empty("projection_id", projection_id)?;
    non_empty("amount", amount)?;
    Ok(())
}

pub fn validate_pickup_location(name: &str, enterprise_id: &str) -> Result<(), String> {
    non_empty("name", name)?;
    non_empty("enterprise_id", enterprise_id)?;
    Ok(())
}

pub fn validate_profile(email: &str) -> Result<(), String> {
    non_empty("email", email)
}

pub fn validate_farmers_cluster(name: &str, region: &str) -> Result<(), String> {
    non_empty("name", name)?;
    non_empty("region", region)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_rules() {
        assert!(validate_enterprise("Farm", "farm").is_ok());
        assert!(validate_enterprise("", "farm").is_err()); // TC-06
        assert!(validate_enterprise("Farm", "").is_err()); // TC-07
    }

    #[test]
    fn campaign_rules() {
        assert!(validate_campaign("Harvest", "ent-1", "5000.00", 100, 1, 2).is_ok());
        assert!(validate_campaign("Harvest", "ent-1", "5000.00", 0, 1, 2).is_err()); // TC-09
        assert!(validate_campaign("Harvest", "ent-1", "5000.00", -1, 1, 2).is_err()); // TC-10
        assert!(validate_campaign("Harvest", "ent-1", "5000.00", 10, 2, 2).is_err()); // TC-11
        assert!(validate_campaign("Harvest", "ent-1", "0", 10, 1, 2).is_err()); // TC-12
        assert!(validate_campaign("", "ent-1", "5000.00", 10, 1, 2).is_err()); // TC-13
    }

    #[test]
    fn contribution_rules() {
        assert!(validate_contribution("c1", 3, "0.1", "0.30").is_ok());
        assert!(validate_contribution("c1", 0, "0.1", "0.00").is_err()); // TC-18
        assert!(validate_contribution("c1", -1, "0.1", "0.00").is_err()); // TC-19
    }

    #[test]
    fn payment_rules() {
        assert!(validate_payment("0.01").is_ok());
        assert!(validate_payment("100.00").is_ok());
        assert!(validate_payment("0").is_err());
        assert!(validate_payment("-1.00").is_err());
        assert!(validate_payment("").is_err());
        assert!(validate_payment("abc").is_err());
    }

    #[test]
    fn other_rules() {
        assert!(validate_batch("B", "c1", "Carrot").is_ok());
        assert!(validate_batch("B", "c1", "").is_err()); // TC-23
        assert!(validate_batch_report("b1", "Sown", "Good").is_ok());
        assert!(validate_batch_report("b1", "", "Good").is_err()); // TC-25
        assert!(validate_projection("c1", 5).is_err()); // TC-30
        assert!(validate_projection("c1", 6).is_ok()); // TC-31
        assert!(validate_profile("").is_err()); // TC-28
        assert!(validate_farmers_cluster("C", "R").is_ok());
        assert!(validate_farmers_cluster("", "R").is_err());
    }
}