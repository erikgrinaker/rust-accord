//! State machine that applies transactions.

use std::fmt::Display;

use crate::clock::Timestamp;
use crate::topology::ShardID;

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

        // Compile-time check that the timestamp fields fit in the expected bit widths below.
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

/// A transaction that can be ordered and executed by Accord.
pub trait Transaction {
    /// Key type used by the topology to derive the transaction's participating shards.
    type ShardingKey;
    /// Read gathered from a shard during execution.
    type Read;
    /// Deterministic output produced once all shard reads have been gathered.
    ///
    /// TODO: add error handling.
    type Output;

    /// Returns the sharding keys involved in the transaction. This is used to determine the
    /// transaction's participating shards, and must be stable for its entire lifetime.
    fn sharding_keys(&self) -> impl Iterator<Item = Self::ShardingKey>;

    /// Executes the transaction from the reads gathered from all shards.
    ///
    /// Implementations must be deterministic so that any coordinator presented with the same
    /// transaction and the same shard reads produces the same output.
    fn execute(&self, reads: impl IntoIterator<Item = (ShardID, Self::Read)>) -> Self::Output;
}

/// Deterministic state machine that applies committed Accord transactions.
///
/// A state machine provides the shard-local operations Accord needs during consensus and execution:
/// deciding whether two transactions conflict, reading local state for a transaction, and applying
/// the committed output at the chosen execution timestamp.
pub trait StateMachine {
    /// Transaction type accepted by this state machine.
    type Txn: Transaction;
    /// Error returned when applying a transaction fails.
    type Error;

    /// Returns whether two transactions conflict, meaning their execution order matters.
    fn conflicts(&self, lhs: &Self::Txn, rhs: &Self::Txn) -> bool;

    /// Reads this replica's local state for a committed transaction before execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the state machine cannot read the local state needed for the
    /// transaction.
    fn read(&self, txn: &Self::Txn) -> Result<<Self::Txn as Transaction>::Read, Self::Error>;

    /// Applies a committed transaction output at the chosen execution timestamp.
    ///
    /// The output is supplied rather than returned because Accord executes the transaction after
    /// gathering reads, then replicates and applies that already-determined result.
    ///
    /// # Errors
    ///
    /// Returns an error if the transaction cannot be applied to the current state.
    fn apply(
        &mut self,
        txn: &Self::Txn,
        timestamp: Timestamp,
        output: &<Self::Txn as Transaction>::Output,
    ) -> Result<(), Self::Error>;
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

    /// Tests that the xor-mul-xor construction remains unique over the entire `u16` domain.
    #[test]
    fn xor_mul_xor_u16_unique() {
        const ODD_MULTIPLIER: u16 = 0x9e37;

        fn scramble(mut bits: u16) -> u16 {
            bits ^= bits >> 8;
            bits = bits.wrapping_mul(ODD_MULTIPLIER);
            bits ^= bits >> 8;
            bits
        }

        // Exhaustive check over all 65,536 inputs. A collision would violate uniqueness.
        let mut seen = vec![false; u16::MAX as usize + 1];
        for input in 0..=u16::MAX {
            let output = scramble(input);
            let is_seen = &mut seen[usize::from(output)];
            assert!(!*is_seen, "collision for input {input} output {output}");
            *is_seen = true;
        }

        assert!(seen.iter().all(|v| *v), "mapping is not surjective over u16");
    }
}
