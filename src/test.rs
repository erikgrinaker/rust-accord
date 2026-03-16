//! Shared test helpers for unit and integration tests.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;

use crate::command::{CommandRecord, CommandStore};
use crate::error::IoError;
use crate::state::{Transaction, TxnID};
use crate::topology::ShardID;

/// An in-memory [`CommandStore`] backed by a [`BTreeMap`].
#[derive(Default)]
pub struct MemoryCommandStore<T: Transaction> {
    map: RwLock<BTreeMap<TxnID, CommandRecord<T>>>,
}

impl<T: Transaction> MemoryCommandStore<T> {
    /// Creates an empty command store.
    #[must_use]
    pub fn new() -> Self {
        Self { map: RwLock::default() }
    }
}

impl<T: Transaction> CommandStore<T> for MemoryCommandStore<T> {
    type ScanIterator = std::vec::IntoIter<Result<CommandRecord<T>, IoError>>;

    fn delete(&self, txn_id: TxnID) -> Result<(), IoError> {
        self.map.write().expect("rwlock poisoned").remove(&txn_id);
        Ok(())
    }

    fn get(&self, txn_id: TxnID) -> Result<Option<CommandRecord<T>>, IoError> {
        Ok(self.map.read().expect("rwlock poisoned").get(&txn_id).cloned())
    }

    fn set(&self, record: CommandRecord<T>) -> Result<(), IoError> {
        self.map.write().expect("rwlock poisoned").insert(record.txn_id, record);
        Ok(())
    }

    fn scan(&self, shards: &BTreeSet<ShardID>) -> Self::ScanIterator {
        self.map
            .read()
            .expect("rwlock poisoned")
            .values()
            .filter(|record| !record.participants.is_disjoint(shards))
            .cloned()
            .map(Ok)
            .collect::<Vec<_>>()
            .into_iter()
    }
}
