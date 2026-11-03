//! Dependency-free FNV-1a 64-bit hashing.
//!
//! Good enough for cache invalidation and cache keys — we never depend
//! on it for security, so there is no need for a crypto crate here.

pub fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub fn fnv1a_hex(data: &[u8]) -> String {
    format!("{:016x}", fnv1a(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // FNV-1a of "foobar" is documented in the reference implementation.
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn hex_is_fixed_width() {
        assert_eq!(fnv1a_hex(&[]).len(), 16);
    }
}
