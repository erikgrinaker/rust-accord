//! Integration test implementing a simple KV store on top of Accord.

use std::collections::{HashMap, HashSet};

use accord::error::{StateError, TxnError};
use accord::{State as _, Transaction as _, WorkingSet as _};

/// Key type for the KV store.
type Key = String;
/// Value type for the KV store.
type Value = String;

/// An operation executed by [`KVTxn`].
#[derive(Clone, Debug, Eq, PartialEq)]
enum KvOp {
    Delete(Key),
    Get(Key),
    Set(Key, Value),
}

impl KvOp {
    /// Returns the key targeted by the operation.
    fn key(&self) -> &Key {
        match self {
            Self::Delete(key) | Self::Get(key) | Self::Set(key, _) => key,
        }
    }

    /// Returns whether the operation mutates the key.
    fn is_write(&self) -> bool {
        match self {
            Self::Delete(_) | Self::Set(..) => true,
            Self::Get(_) => false,
        }
    }
}

/// Client-visible output of a [`KVOp`].
#[derive(Clone, Debug, Eq, PartialEq)]
enum KvOutput {
    Delete(bool),
    Get(Option<Value>),
    Set(bool),
}

/// Prepared working set for a [`KvTxn`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct KvWorkingSet {
    reads: HashSet<Key>,
    writes: HashSet<Key>,
}

impl accord::WorkingSet<KvTxn> for KvWorkingSet {
    fn read_set(&self) -> HashSet<Key> {
        self.reads.clone()
    }

    fn write_set(&self) -> HashSet<Key> {
        self.writes.clone()
    }
}

/// Key/value transaction which executes a sequence of operations.
#[derive(Clone, Debug)]
struct KvTxn {
    ops: Vec<KvOp>,
}

impl KvTxn {
    /// Creates a transaction from an ordered sequence of operations.
    fn new(ops: Vec<KvOp>) -> Self {
        Self { ops }
    }
}

impl accord::Transaction for KvTxn {
    type ShardKey = Key;
    type ShardValue = Option<Value>;
    type ShardRead = ();
    type ShardUpdate = Option<Value>;
    type WorkingSet = KvWorkingSet;
    type Output = Vec<KvOutput>;

    fn prepare(&self) -> Result<Self::WorkingSet, TxnError> {
        let mut working_set = KvWorkingSet::default();
        for op in &self.ops {
            let key = op.key();
            working_set.reads.insert(key.clone());
            if op.is_write() {
                working_set.writes.insert(key.clone());
            }
        }
        Ok(working_set)
    }

    fn reads(&self) -> HashMap<Key, ()> {
        self.ops.iter().map(|op| (op.key().clone(), ())).collect()
    }

    fn execute(
        self,
        reads: HashMap<Key, Option<Value>>,
    ) -> Result<accord::Outcome<Self>, TxnError> {
        let mut updates = HashMap::new();
        let mut output = Vec::with_capacity(self.ops.len());

        for op in self.ops {
            // Read the key's current value, including reading our own writes. Errors if no value
            // is found, since all keys should have been read during prepare.
            let key = op.key();
            let current = updates
                .get(key)
                .or_else(|| reads.get(key))
                .ok_or_else(|| TxnError(format!("missing read for key {key}")))?;

            match op {
                KvOp::Delete(key) => {
                    let exists = current.is_some();
                    updates.insert(key, None);
                    output.push(KvOutput::Delete(exists));
                }
                KvOp::Get(_key) => {
                    output.push(KvOutput::Get(current.clone()));
                }
                KvOp::Set(key, value) => {
                    let exists = current.is_some();
                    updates.insert(key, Some(value));
                    output.push(KvOutput::Set(exists));
                }
            }
        }

        Ok(accord::Outcome { updates, output })
    }
}

/// Key/value store as an Accord state machine.
#[derive(Debug, Default)]
struct KVState {
    inner: HashMap<Key, Value>,
}

impl accord::State for KVState {
    type Txn = KvTxn;

    fn read(&self, reads: HashMap<Key, ()>) -> Result<HashMap<Key, Option<Value>>, StateError> {
        Ok(reads.keys().map(|key| (key.clone(), self.inner.get(key).cloned())).collect())
    }

    fn update(&mut self, updates: HashMap<Key, Option<Value>>) -> Result<(), StateError> {
        for (key, value) in updates {
            match value {
                Some(value) => self.inner.insert(key, value),
                None => self.inner.remove(&key),
            };
        }
        Ok(())
    }
}

#[test]
fn kv_txn_conflicts() {
    let read = KvTxn::new(vec![KvOp::Get("alpha".into())]).prepare().expect("prepare read");
    let write = KvTxn::new(vec![KvOp::Set("alpha".into(), "value".into())])
        .prepare()
        .expect("prepare write");

    // Reads don't conflict.
    assert!(!read.conflicts(&read));

    // Writes conflict with reads and writes.
    assert!(read.conflicts(&write));
    assert!(write.conflicts(&read));
    assert!(write.conflicts(&write));

    // Non-overlapping keys don't conflict.
    let a = KvTxn::new(vec![KvOp::Set("a".into(), "value".into())]).prepare().expect("prepare a");
    let b = KvTxn::new(vec![KvOp::Set("b".into(), "value".into())]).prepare().expect("prepare b");

    assert!(!a.conflicts(&b));
    assert!(!b.conflicts(&a));
}

/// Tests basic execution of a [`KvTxn`], driving it manually.
/// TODO: Accord should drive this.
#[test]
#[allow(clippy::zero_sized_map_values)]
fn kv_txn_execute() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = KVState {
        inner: HashMap::from([("alpha".into(), "value".into()), ("beta".into(), "old".into())]),
    };
    let txn = KvTxn::new(vec![
        KvOp::Get("alpha".into()),
        KvOp::Set("beta".into(), "new".into()),
        KvOp::Get("beta".into()),
        KvOp::Delete("alpha".into()),
        KvOp::Get("alpha".into()),
        KvOp::Delete("gamma".into()),
    ]);

    // Prepare the transaction, which gathers the working set.
    let working_set = txn.prepare()?;
    assert_eq!(
        working_set,
        KvWorkingSet {
            reads: HashSet::from(["alpha".into(), "beta".into(), "gamma".into()]),
            writes: HashSet::from(["alpha".into(), "beta".into(), "gamma".into()]),
        }
    );

    // Gather the reads.
    let reads = state.read(txn.reads())?;

    // Execute the transaction and verify the outcome.
    let outcome = txn.execute(reads)?;
    assert_eq!(
        outcome.updates,
        HashMap::from([
            ("alpha".into(), None),
            ("beta".into(), Some("new".into())),
            ("gamma".into(), None),
        ]),
    );
    assert_eq!(
        outcome.output,
        vec![
            KvOutput::Get(Some("value".into())),
            KvOutput::Set(true),
            KvOutput::Get(Some("new".into())),
            KvOutput::Delete(true),
            KvOutput::Get(None),
            KvOutput::Delete(false),
        ],
    );

    // Apply the updates to the state machine.
    state.update(outcome.updates)?;
    assert_eq!(state.inner, HashMap::from([("beta".into(), "new".into())]));

    Ok(())
}
