//! State machine that applies transactions.

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::hash::Hash;

use crate::error::{StateError, TxnError};
use crate::time::Timestamp;

/// Stable, globally unique identifier for a transaction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
pub trait Transaction: Clone {
    /// Key used to derive the transaction's participating shards, and as a key for
    /// [`Self::ShardRead`] and [`Self::ShardUpdate`] operations.
    type ShardKey: Clone + Debug + Display + Eq + Hash + Ord + PartialEq + PartialOrd;
    /// Value returned for a [`Self::ShardRead`] operation and used for execution.
    type ShardValue: Debug;
    /// Read operation to submit to a participating shard prior to execution.
    type ShardRead: Debug;
    /// Deterministic state update for a participating shard produced during execution.
    type ShardUpdate: Clone + Debug;
    /// The transaction's working set. Used to determine participating shards and detect conflicts
    /// between transactions during preparation.
    type WorkingSet: WorkingSet<Self>;
    /// Deterministic client-visible output produced during execution.
    type Output: Debug;

    /// Prepares the transaction and returns its working set.
    ///
    /// # Errors
    ///
    /// Fails if the transaction cannot be prepared, for example if it is malformed or missing
    /// required information. A failed transaction will be discarded.
    fn prepare(&self) -> Result<Self::WorkingSet, TxnError>;

    /// Returns shard reads to submit to participating shards during execution.
    ///
    /// # Errors
    ///
    /// Fails if the transaction can't produce the shard reads needed for execution.
    fn reads(&self) -> Result<ShardReads<Self>, TxnError>;

    /// Executes the transaction with read values gathered from participating shards. Returns
    /// deterministic state machine updates and client-visible output.
    ///
    /// Implementations must be deterministic so that any coordinator presented with the same
    /// transaction and the same shard reads produces the same mutation and output.
    ///
    /// # Errors
    ///
    /// Fails if the transaction can't execute. It will be discarded. Deterministic user-facing
    /// errors should be returned as part of [`Self::Output`] instead.
    fn execute(
        self,
        reads: ShardValues<Self>,
    ) -> Result<(ShardUpdates<Self>, Self::Output), TxnError>;
}

/// Shard keys.
pub type ShardKeys<T> = HashSet<<T as Transaction>::ShardKey>;

/// Reads dispatches to shards prior to transaction execution.
pub type ShardReads<T> = HashMap<<T as Transaction>::ShardKey, <T as Transaction>::ShardRead>;

/// Values gathered from shards via [`Transaction::ShardRead`] operations.
pub type ShardValues<T> = HashMap<<T as Transaction>::ShardKey, <T as Transaction>::ShardValue>;

/// Updates to apply to shards via [`StateMachine::update`] following transaction execution.
pub type ShardUpdates<T> = HashMap<<T as Transaction>::ShardKey, <T as Transaction>::ShardUpdate>;

/// The working set of a transaction, returned by [`Transaction::prepare`]. Used to determine
/// participating shards and detect conflicts between transactions.
pub trait WorkingSet<T: Transaction> {
    /// Determines whether this transaction conflicts with another transaction. By default, checks
    /// if either transaction's write set overlaps with the other transaction's working set, which
    /// is a common heuristic, but transactions can override it with their own conflict logic.
    fn conflicts(&self, other: &Self) -> bool {
        let lhs_writes = self.update_keys();
        let rhs_writes = other.update_keys();

        // Fast path: no writes.
        if lhs_writes.is_empty() && rhs_writes.is_empty() {
            return false;
        }

        !(lhs_writes.is_disjoint(&other.keys()) && rhs_writes.is_disjoint(&self.keys()))
    }

    /// Generates a new working set for the given keys. This is used to partition the working set
    /// across shards during preparation.
    fn for_keys(&self, keys: &ShardKeys<T>) -> Self;

    /// All shard keys that will be accessed during execution. The union of the read and write keys.
    fn keys(&self) -> ShardKeys<T> {
        let mut work_set = self.read_keys();
        work_set.extend(self.update_keys());
        work_set
    }

    /// Shard keys that will be read during execution.
    fn read_keys(&self) -> ShardKeys<T>;

    /// Shard keys that will be updated during execution.
    fn update_keys(&self) -> ShardKeys<T>;
}

/// Deterministic state machine that applies committed Accord transactions.
///
/// A state machine provides the shard-local operations Accord needs during execution: reading local
/// state for a transaction and applying the committed apply payload at the chosen execution
/// timestamp.
pub trait State {
    /// Transaction type accepted by this state machine.
    type Txn: Transaction;

    /// Reads this replica's local state for a committed transaction before execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the state machine cannot read the local state needed for the
    /// transaction.
    fn read(&self, reads: ShardReads<Self::Txn>) -> Result<ShardValues<Self::Txn>, StateError>;

    /// Applies a committed transaction update.
    ///
    /// # Errors
    ///
    /// Returns an error if the transaction cannot be applied to the current state.
    fn update(&mut self, updates: ShardUpdates<Self::Txn>) -> Result<(), StateError>;
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
    fn txn_id_display() {
        let txn_id = TxnID(0x7ade_a432_46a1_7d0e_64f2_d490_907d_f5a6);
        assert_eq!(txn_id.to_string(), "7adea43246a17d0e64f2d490907df5a6");

        let txn_id = TxnID(0);
        assert_eq!(txn_id.to_string(), "00000000000000000000000000000000");
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
