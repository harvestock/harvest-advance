//! Server-generated identifier helpers (SPEC §3.1).
//!
//! The system generates its own `id` values as 128 random bits rendered as 32
//! lowercase hex characters. The random bytes come from the host (`random_bytes`)
//! in the coordinator; this crate only formats and checks them.

/// Render 16 bytes (128 bits) as 32 lowercase hex characters.
pub fn hex32(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// True when `s` is exactly 32 lowercase hex characters.
pub fn is_hex32(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_128_bits_as_32_lower_hex() {
        let bytes = [0xab; 16];
        let s = hex32(&bytes);
        assert_eq!(s.len(), 32);
        assert_eq!(s, "abababababababababababababababab");
        assert!(is_hex32(&s));
    }

    #[test]
    fn rejects_bad_shapes() {
        assert!(!is_hex32(""));
        assert!(!is_hex32("ABCDEF"));
        assert!(!is_hex32(&"a".repeat(31)));
        assert!(!is_hex32(&"g".repeat(32)));
    }
}