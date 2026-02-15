//! CRC16-XMODEM implementation for Redis Cluster slot calculation.
//!
//! Redis Cluster uses CRC16 with the XMODEM polynomial (0x1021) to map keys
//! to one of 16384 hash slots.

/// Number of hash slots in a Redis Cluster.
pub const SLOT_COUNT: u16 = 16384;

/// CRC16-XMODEM lookup table (polynomial 0x1021).
static CRC16_TABLE: [u16; 256] = {
    let mut table = [0u16; 256];
    let mut i = 0u16;
    while i < 256 {
        let mut crc = i << 8;
        let mut j = 0;
        while j < 8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

/// Compute CRC16-XMODEM checksum of `data`.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        let idx = ((crc >> 8) ^ (byte as u16)) as usize;
        crc = (crc << 8) ^ CRC16_TABLE[idx];
    }
    crc
}

/// Extract the hash tag from a Redis key.
///
/// If the key contains `{...}` with at least one character between the first `{`
/// and the first subsequent `}`, the content between them is the hash tag.
/// Otherwise, the entire key is used.
///
/// Returns the portion of the key that should be hashed.
pub fn extract_hash_tag(key: &[u8]) -> &[u8] {
    if let Some(open) = key.iter().position(|&b| b == b'{') {
        // Look for '}' after the '{', must have at least 1 char between
        if let Some(close_offset) = key[open + 1..].iter().position(|&b| b == b'}') {
            if close_offset > 0 {
                return &key[open + 1..open + 1 + close_offset];
            }
        }
    }
    key
}

/// Calculate the Redis Cluster hash slot for a given key.
///
/// Extracts the hash tag (if any) and computes `CRC16(tag) % 16384`.
pub fn hash_slot(key: &[u8]) -> u16 {
    let tag = extract_hash_tag(key);
    crc16(tag) % SLOT_COUNT
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CRC16 known vectors ──
    // These match the Redis source (src/crc16.c) test vectors.

    #[test]
    fn crc16_empty() {
        assert_eq!(crc16(b""), 0);
    }

    #[test]
    fn crc16_known_values() {
        // Test with the canonical "123456789" string
        // CRC16-XMODEM of "123456789" = 0x31C3
        assert_eq!(crc16(b"123456789"), 0x31C3);
    }

    #[test]
    fn crc16_single_byte() {
        // Just verify deterministic
        let c1 = crc16(b"a");
        let c2 = crc16(b"a");
        assert_eq!(c1, c2);
        assert_ne!(crc16(b"a"), crc16(b"b"));
    }

    #[test]
    fn crc16_different_inputs() {
        assert_ne!(crc16(b"hello"), crc16(b"world"));
    }

    // ── Hash tag extraction ──

    #[test]
    fn hash_tag_simple() {
        assert_eq!(extract_hash_tag(b"{user:1000}.following"), b"user:1000");
    }

    #[test]
    fn hash_tag_no_braces() {
        assert_eq!(extract_hash_tag(b"mykey"), b"mykey");
    }

    #[test]
    fn hash_tag_empty_braces() {
        // {} has nothing between braces → use entire key
        assert_eq!(extract_hash_tag(b"{}mykey"), b"{}mykey");
    }

    #[test]
    fn hash_tag_open_only() {
        // { without } → use entire key
        assert_eq!(extract_hash_tag(b"{mykey"), b"{mykey");
    }

    #[test]
    fn hash_tag_close_before_open() {
        // } before { → use entire key
        assert_eq!(extract_hash_tag(b"}mykey{tag}"), b"tag");
    }

    #[test]
    fn hash_tag_multiple_braces() {
        // First valid {…} pair wins
        assert_eq!(extract_hash_tag(b"{a}{b}"), b"a");
    }

    #[test]
    fn hash_tag_nested() {
        // First { to first } after it
        assert_eq!(extract_hash_tag(b"{{nested}}"), b"{nested");
    }

    #[test]
    fn hash_tag_only_braces() {
        assert_eq!(extract_hash_tag(b"{}"), b"{}");
    }

    #[test]
    fn hash_tag_brace_at_end() {
        assert_eq!(extract_hash_tag(b"key{"), b"key{");
    }

    #[test]
    fn hash_tag_adjacent_braces_with_content() {
        assert_eq!(extract_hash_tag(b"prefix{tag}suffix"), b"tag");
    }

    // ── Hash slot ──

    #[test]
    fn hash_slot_range() {
        // Slot must be in [0, 16383]
        for key in &[b"a".as_ref(), b"z", b"hello", b"key:12345", b""] {
            assert!(hash_slot(key) < SLOT_COUNT);
        }
    }

    #[test]
    fn hash_slot_same_tag_same_slot() {
        let slot1 = hash_slot(b"{user:1000}.following");
        let slot2 = hash_slot(b"{user:1000}.followers");
        let slot3 = hash_slot(b"{user:1000}.name");
        assert_eq!(slot1, slot2);
        assert_eq!(slot2, slot3);
    }

    #[test]
    fn hash_slot_different_keys_can_differ() {
        // Not guaranteed but highly likely for different keys
        let slot1 = hash_slot(b"key1");
        let slot2 = hash_slot(b"key2");
        // These happen to differ but the real test is that they're valid
        assert!(slot1 < SLOT_COUNT);
        assert!(slot2 < SLOT_COUNT);
    }

    #[test]
    fn hash_slot_without_tag_uses_whole_key() {
        // Without tag, slot is based on entire key
        let slot_full = hash_slot(b"mykey");
        let slot_manual = crc16(b"mykey") % SLOT_COUNT;
        assert_eq!(slot_full, slot_manual);
    }

    #[test]
    fn hash_slot_with_tag_uses_tag_only() {
        let slot_tagged = hash_slot(b"{tag}:rest");
        let slot_tag = crc16(b"tag") % SLOT_COUNT;
        assert_eq!(slot_tagged, slot_tag);
    }

    #[test]
    fn hash_slot_empty_key() {
        // Empty key still computes a valid slot
        assert!(hash_slot(b"") < SLOT_COUNT);
    }
}
