//! Replica-local command metadata and storage abstractions.
//!
//! Accord stores per-transaction state at each replica so that consensus, recovery, and execution
//! can all reason about the same command record. The core fields are the coordinator's
//! [`CommandRecord::proposed_at`], the chosen [`CommandRecord::execute_at`],
//! [`CommandRecord::dependencies`], ballot promises, and the highest phase the command has
//! reached.

use std::collections::HashSet;

use crate::error::IoError;
use crate::state::{ShardUpdates, Transaction, TxnID};
use crate::time::Timestamp;
use crate::topology::{NodeID, ShardID};

/// Recovery ballot for a command.
///
/// Ballots are ordered lexicographically by `(counter, node)` so higher ballots supersede lower
/// ballots during recovery.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Ballot {
    /// Monotonic counter chosen by the recovery coordinator.
    pub counter: u64,
    /// Coordinator that allocated the ballot.
    pub node: NodeID,
}

/// Highest command status reached at this replica.
///
/// The paper reasons about these states with monotonic boolean flags
/// (`PreAccepted`, `Accepted`, `Committed`, `Applied`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CommandStatus {
    /// The replica has recorded a pre-accept vote for the command.
    PreAccepted,
    /// The replica has recorded an accept vote for the command.
    Accepted,
    /// The execution timestamp and dependency set have been committed.
    Committed,
    /// The transaction result has been applied to the local shard state.
    Applied,
}

/// Replica-local record for a transaction being processed by Accord.
#[derive(Clone)]
pub struct CommandRecord<T: Transaction> {
    /// Stable transaction identifier.
    pub txn_id: TxnID,
    /// Transaction payload.
    pub txn: T,
    /// Shards that participate in the transaction.
    ///
    /// INVARIANT: non-empty.
    pub participants: HashSet<ShardID>,
    /// Highest command status reached at this replica.
    pub status: CommandStatus,
    /// Coordinator proposal timestamp.
    pub proposed_at: Timestamp,
    /// Chosen execution timestamp.
    ///
    /// INVARIANT: [`Self::execute_at`] >= [`Self::proposed_at`].
    pub execute_at: Timestamp,
    /// Dependencies that must be visible before execution or apply may proceed.
    ///
    /// Execution filters this set per shard using the dependency commands'
    /// [`CommandRecord::participants`].
    pub dependencies: HashSet<TxnID>,
    /// Highest recovery ballot promised by this replica.
    pub promised_ballot: Ballot,
    /// Ballot that most recently accepted [`Self::execute_at`] and [`Self::dependencies`], if any.
    ///
    /// INVARIANT: this is `Some` if [`Self::status`] is at least [`CommandStatus::Accepted`].
    pub accepted_ballot: Option<Ballot>,
    /// Deterministic shard updates, once computed.
    ///
    /// This may be populated before the record reaches [`CommandStatus::Applied`].
    pub updates: Option<ShardUpdates<T>>,
}

/// Replica-local storage for command records. The store is safe for concurrent access.
pub trait CommandStore<T: Transaction> {
    /// The iterator returned by [`Self::scan`].
    type ScanIterator: Iterator<Item = Result<CommandRecord<T>, IoError>>;

    /// Deletes a command record.
    ///
    /// # Errors
    ///
    /// Fails if there was an IO error.
    fn delete(&self, txn_id: TxnID) -> Result<(), IoError>;

    /// Returns a command record, if present.
    ///
    /// # Errors
    ///
    /// Fails if there was an IO error.
    fn get(&self, txn_id: TxnID) -> Result<Option<CommandRecord<T>>, IoError>;

    /// Writes a command record, replacing any existing record. Uses the record's
    /// [`CommandRecord::txn_id`] as the key.
    ///
    /// # Errors
    ///
    /// Fails if there was an IO error.
    ///
    /// TODO: consider specialized methods that do per-field updates, without rewriting the entire
    /// command record.
    fn set(&self, record: CommandRecord<T>) -> Result<(), IoError>;

    /// Lists commands that intersect the given shards.
    fn scan(&self, shards: &HashSet<ShardID>) -> Self::ScanIterator;
}
