//! Accord wire protocol messages and transport abstractions.
//!
//! The Accord paper splits communication into a consensus path (`PreAccept`, `Accept`, `Commit`)
//! and an execution path (`Read`, `Apply`). This module models those protocol messages so the
//! crate can represent coordinator-to-replica and replica-to-coordinator traffic explicitly.
//!
//! Recovery and reconfiguration messages are intentionally omitted for now.

use std::collections::HashSet;

use crate::command::Ballot;
use crate::error::IoError;
use crate::state::{ShardReads, ShardUpdates, ShardValues, Transaction, TxnID};
use crate::time::Timestamp;
use crate::topology::{NodeID, ShardID, ShardReplica};

/// Dependency set attached to a protocol decision.
pub type Dependencies = HashSet<TxnID>;

/// Participants in a transaction.
pub type Participants = HashSet<ShardID>;

/// Transport endpoint for Accord protocol messages.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Endpoint {
    /// A coordinator node endpoint.
    Node(NodeID),
    /// A specific shard replica endpoint.
    Replica(ShardReplica),
}

/// Routed protocol message delivered through a [`Transport`].
pub struct Envelope<T: Transaction> {
    /// Originating endpoint.
    pub from: Endpoint,
    /// Destination endpoint.
    pub to: Endpoint,
    /// Protocol payload.
    pub message: Message<T>,
}

/// Protocol message exchanged between coordinators and shard replicas.
pub enum Message<T: Transaction> {
    /// Fast-path proposal sent to the electorate of each participating shard.
    PreAccept {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Shards that participate in the transaction.
        participants: Participants,
        /// Coordinator proposal timestamp from the paper's `t0`.
        proposed_at: Timestamp,
        /// Working set used for conflict detection at the destination shard.
        ///
        /// Callers should partition this with [`crate::WorkingSet::for_keys`]
        /// before sending to a shard replica.
        working_set: T::WorkingSet,
    },
    /// Reply to a fast-path proposal.
    PreAcceptOk {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Replica-local timestamp proposal for the command.
        execute_at: Timestamp,
        /// Dependencies observed by the replica.
        dependencies: Dependencies,
    },
    /// Slow-path accept request.
    Accept {
        /// Ballot authorizing the accept round.
        ballot: Ballot,
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Shards that participate in the transaction.
        participants: Participants,
        /// Coordinator proposal timestamp from the paper's `t0`.
        proposed_at: Timestamp,
        /// Working set used for conflict detection at the destination shard.
        ///
        /// Callers should partition this with [`crate::WorkingSet::for_keys`]
        /// before sending to a shard replica.
        working_set: T::WorkingSet,
        /// Chosen execution timestamp.
        execute_at: Timestamp,
        /// Transactions that must be visible before execution or apply.
        dependencies: Dependencies,
    },
    /// Reply to an accept round.
    AcceptOk {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Dependencies observed by the replica for the accepted decision.
        dependencies: Dependencies,
    },
    /// Commit notification after consensus.
    Commit {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Shards that participate in the transaction.
        ///
        /// INVARIANT: non-empty.
        participants: Participants,
        /// Coordinator proposal timestamp from the paper's `t0`.
        proposed_at: Timestamp,
        /// Working set used for conflict detection at the destination shard.
        ///
        /// Callers should partition this with [`crate::WorkingSet::for_keys`]
        /// before sending to a shard replica.
        working_set: T::WorkingSet,
        /// Chosen execution timestamp.
        execute_at: Timestamp,
        /// Transactions that must be visible before execution or apply.
        dependencies: Dependencies,
    },
    /// Shard-local read request during execution.
    Read {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Chosen execution timestamp.
        execute_at: Timestamp,
        /// Dependencies that overlap the target shard.
        dependencies: Dependencies,
        /// Shard-local reads for the replica that owns them.
        ///
        /// INVARIANT: all keys in this map belong to the target shard.
        reads: ShardReads<T>,
    },
    /// Shard-local read reply during execution.
    ReadOk {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Values read from the target shard.
        values: ShardValues<T>,
    },
    /// Apply notification carrying the deterministic execution outcome.
    Apply {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Chosen execution timestamp.
        execute_at: Timestamp,
        /// Dependencies that overlap the target shard.
        dependencies: Dependencies,
        /// Shard updates produced by execution.
        updates: ShardUpdates<T>,
        /// Deterministic client-visible output.
        output: T::Output,
    },
    /// Negative acknowledgement for an obsolete ballot.
    Nack {
        /// Stable transaction identifier.
        txn_id: TxnID,
        /// Highest ballot promised by the replica.
        promised_ballot: Ballot,
    },
}

/// Point-to-point transport used to deliver Accord protocol messages.
///
/// The paper assumes a partially synchronous network with crash-stop failures, loosely synchronized
/// clocks, and no FIFO delivery guarantee. Implementations must therefore tolerate message
/// reordering, duplication, delay, and loss, and higher layers must discard stale messages once a
/// newer phase or ballot has been observed.
///
/// TODO: consider making this async.
pub trait Transport<T: Transaction>: Send + Sync {
    /// Sends an outbound message.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport could not enqueue or emit the message.
    fn send(&self, envelope: Envelope<T>) -> Result<(), IoError>;

    /// Receives the next inbound message.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport could not receive or decode a message.
    fn recv(&self) -> Result<Envelope<T>, IoError>;
}
