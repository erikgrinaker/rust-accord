//! Shard topology types.
//!
//! Accord assumes a pre-existing topology that partitions data into shards and assigns each shard
//! to a replica-set. Transactions are routed to the set of shards they may touch before consensus
//! begins, and topology changes are modeled as configuration changes across epochs.

use std::collections::HashSet;

/// Configuration epoch for shard membership and partitioning decisions.
pub type Epoch = u64;

/// Stable identifier for a node in the cluster.
pub type NodeID = u32;

/// Stable identifier for a shard.
pub type ShardID = u64;

/// Membership metadata for a shard.
pub struct Shard {
    /// Stable and unique identifier for the shard.
    pub id: ShardID,
    /// Replicas that host this shard.
    ///
    /// INVARIANT: this set is non-empty.
    /// INVARIANT: all replicas have [`ShardReplica::shard_id`] equal to [`Self::id`].
    pub replicas: HashSet<ShardReplica>,
}

/// A shard replica instance hosted by a node. A node can only host one replica of a given shard
/// (but it can host replicas of multiple shards).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShardReplica {
    /// Shard hosted by this replica instance.
    pub shard_id: ShardID,
    /// Node hosting this replica instance.
    pub node_id: NodeID,
}

/// Versioned shard topology used to map sharding keys to shards.
///
/// TODO: support reconfiguration.
pub trait Topology {
    /// Key type used by the partitioning scheme.
    type ShardKey;

    /// Returns the configuration epoch for this topology.
    fn epoch(&self) -> Epoch;

    /// Returns the metadata for the given shard, if it exists in this topology.
    fn shard(&self, shard_id: ShardID) -> Option<&Shard>;

    /// Maps a key to the shard that owns it in this topology.
    fn key_to_shard(&self, key: &Self::ShardKey) -> ShardID;
}
