//! Exact decimal arithmetic for money.
//!
//! Money on the wire is a decimal string with a `.` separator (SPEC §3.1). The
//! rewrite must never use binary floating point for money, because that turns
//! `0.1 * 3` into `0.30000000000000004` (SPEC §5.4, TC-20). A value is stored as
//! an integer mantissa plus a base-10 scale: `value = mantissa / 10^scale`.

use std::cmp::Ordering;

/// An exact base-10 decimal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decimal {
    /// Signed integer mantissa.
    pub mantissa: i128,
    /// Number of digits after the decimal point.
    pub scale: u32,
}

fn pow10(exp: u32) -> i128 {
    10i128.pow(exp)
}

impl Decimal {
    /// Zero.
    pub fn zero() -> Self {
        Decimal {
            mantissa: 0,
            scale: 0,
        }
    }

    /// Build from a mantissa and scale without validation (used internally).
    pub fn from_parts(mantissa: i128, scale: u32) -> Self {
        Decimal { mantissa, scale }
    }

    /// Parse a decimal string such as `"5000.00"`, `"0.1"`, `"-12"`, `"+3.5"`.
    ///
    /// Rejects empty input, non-digit characters, and more than one `.`.
    pub fn parse(input: &str) -> Result<Self, String> {
        let s = input.trim();
        if s.is_empty() {
            return Err("decimal string is empty".to_string());
        }

        let (negative, body) = match s.as_bytes()[0] {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        if body.is_empty() {
            return Err(format!("invalid decimal: {input:?}"));
        }

        let mut pieces = body.splitn(2, '.');
        let int_part = pieces.next().unwrap_or("");
        let frac_part = pieces.next();

        if int_part.is_empty() && frac_part.map_or(true, |f| f.is_empty()) {
            return Err(format!("invalid decimal: {input:?}"));
        }

        let mut mantissa: i128 = 0;
        for c in int_part.bytes() {
            if !c.is_ascii_digit() {
                return Err(format!("invalid decimal: {input:?}"));
            }
            mantissa = mantissa
                .checked_mul(10)
                .and_then(|m| m.checked_add((c - b'0') as i128))
                .ok_or_else(|| format!("decimal out of range: {input:?}"))?;
        }

        let mut scale: u32 = 0;
        if let Some(frac) = frac_part {
            for c in frac.bytes() {
                if !c.is_ascii_digit() {
                    return Err(format!("invalid decimal: {input:?}"));
                }
                mantissa = mantissa
                    .checked_mul(10)
                    .and_then(|m| m.checked_add((c - b'0') as i128))
                    .ok_or_else(|| format!("decimal out of range: {input:?}"))?;
                scale += 1;
            }
        }

        if negative {
            mantissa = -mantissa;
        }

        Ok(Decimal { mantissa, scale })
    }

    /// Multiply by a signed integer, keeping the same scale.
    pub fn mul_i64(&self, factor: i64) -> Self {
        Decimal {
            mantissa: self.mantissa * factor as i128,
            scale: self.scale,
        }
    }

    /// Add two decimals, aligning scales.
    pub fn add(&self, other: &Decimal) -> Self {
        let scale = self.scale.max(other.scale);
        let a = self.mantissa * pow10(scale - self.scale);
        let b = other.mantissa * pow10(scale - other.scale);
        Decimal {
            mantissa: a + b,
            scale,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.mantissa == 0
    }

    pub fn is_positive(&self) -> bool {
        self.mantissa > 0
    }

    pub fn is_negative(&self) -> bool {
        self.mantissa < 0
    }

    /// Compare two decimals by numeric value.
    pub fn cmp_value(&self, other: &Decimal) -> Ordering {
        let scale = self.scale.max(other.scale);
        let a = self.mantissa * pow10(scale - self.scale);
        let b = other.mantissa * pow10(scale - other.scale);
        a.cmp(&b)
    }

    /// Render with exactly `places` fractional digits, rounding half away from
    /// zero when the value has more precision than requested.
    pub fn to_fixed(&self, places: u32) -> String {
        let scaled: i128 = if self.scale <= places {
            self.mantissa * pow10(places - self.scale)
        } else {
            let divisor = pow10(self.scale - places);
            let mut quotient = self.mantissa / divisor;
            let remainder = self.mantissa % divisor;
            // Round half away from zero.
            if remainder.abs() * 2 >= divisor {
                if self.mantissa < 0 {
                    quotient -= 1;
                } else {
                    quotient += 1;
                }
            }
            quotient
        };

        let negative = scaled < 0;
        let digits = scaled.unsigned_abs().to_string();
        let body = if places == 0 {
            digits
        } else {
            let places = places as usize;
            let padded = if digits.len() <= places {
                format!("{}{}", "0".repeat(places + 1 - digits.len()), digits)
            } else {
                digits
            };
            let split = padded.len() - places;
            format!("{}.{}", &padded[..split], &padded[split..])
        };

        if negative && scaled != 0 {
            format!("-{body}")
        } else {
            body
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        assert_eq!(Decimal::parse("5000.00").unwrap().to_fixed(2), "5000.00");
        assert_eq!(Decimal::parse("0.1").unwrap().to_fixed(2), "0.10");
        assert_eq!(Decimal::parse("-12").unwrap().to_fixed(2), "-12.00");
        assert_eq!(Decimal::parse("0").unwrap().to_fixed(2), "0.00");
    }

    #[test]
    fn tc20_exact_decimal() {
        // The whole point of TC-20: 0.1 * 3 == "0.30", not 0.30000000000000004.
        let unit = Decimal::parse("0.1").unwrap();
        let total = unit.mul_i64(3).to_fixed(2);
        assert_eq!(total, "0.30");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse("abc").is_err());
        assert!(Decimal::parse("1.2.3").is_err());
        assert!(Decimal::parse("1a").is_err());
    }

    #[test]
    fn add_and_compare() {
        let a = Decimal::parse("0.1").unwrap();
        let b = Decimal::parse("0.2").unwrap();
        assert_eq!(a.add(&b).to_fixed(2), "0.30");
        assert_eq!(a.cmp_value(&b), Ordering::Less);
        assert!(Decimal::parse("0").unwrap().is_zero());
        assert!(Decimal::parse("0.01").unwrap().is_positive());
    }
}