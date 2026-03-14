//! Transaction types.

use std::fmt::Display;

use crate::clock::Timestamp;

/// Stable, globally unique identifier for a transaction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct TxnID(u128);

impl From<Timestamp> for TxnID {
    /// Constructs a transaction ID from a logical timestamp, typically the proposal timestamp.
    /// Timestamps are convenient transaction ID seeds because they are globally unique.
    ///
    /// The transaction ID should not expose timestamp order directly, since a transaction's
    /// timestamp may change during retries or recovery. To avoid accidental reliance on that order,
    /// the packed timestamp bits are scrambled with a reversible permutation.
    fn from(timestamp: Timestamp) -> Self {
        // The 64-bit golden ratio constant has good bit-mixing properties. It is also odd, so
        // `wrapping_mul` by it is invertible modulo 2^128.
        const GOLDEN_RATIO: u128 = 0x9e37_79b9_7f4a_7c15;

        // Compile-time check that the timestamp fields fit in the expected bit widths above.
        let _: u64 = timestamp.time;
        let _: u32 = timestamp.seq;
        let _: u32 = timestamp.node;

        // Pack the timestamp fields into a 128-bit integer.
        let mut bits = (u128::from(timestamp.time) << 64)
            | (u128::from(timestamp.seq) << 32)
            | u128::from(timestamp.node);

        // Scramble the bits with a reversible permutation so `TxnID` doesn't retain the timestamp
        // order yet remains unique. `bits ^= bits >> 64` keeps the upper 64 bits unchanged and XORs
        // them into the lower 64 bits, so it is reversible. `wrapping_mul` is modulo 2^128, and an
        // odd multiplier is invertible modulo 2^128.
        bits ^= bits >> 64;
        bits = bits.wrapping_mul(GOLDEN_RATIO);
        bits ^= bits >> 64;

        Self(bits)
    }
}

/// Displays a transaction ID as hex.
impl Display for TxnID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that [`TxnID::from`] deterministically scrambles the timestamp fields.
    #[test]
    fn txn_id_from_timestamp_scrambles_fields() {
        let timestamp =
            Timestamp { time: 0x0001_0203_0405_0607, seq: 0x0809_0a0b, node: 0x0c0d_0e0f };
        let txn_id = TxnID::from(timestamp);
        assert_ne!(txn_id.0, 0x0001_0203_0405_0607_0809_0a0b_0c0d_0e0f);
        assert_eq!(txn_id.0, 0x7ade_a432_46a1_7d0e_64f2_d490_907d_f5a6);
    }

    /// Tests that [`TxnID`] displays the ID as hex.
    #[test]
    fn txn_id_display_formats_hex_segments() {
        let txn_id = TxnID(0x7ade_a432_46a1_7d0e_64f2_d490_907d_f5a6);
        assert_eq!(txn_id.to_string(), "7adea43246a17d0e64f2d490907df5a6");
    }
}
