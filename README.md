# rust-accord

Aspirational Rust prototype of the Accord consensus protocol.

* [Accord paper](https://cwiki.apache.org/confluence/download/attachments/188744725/Accord.pdf)
* [Cassandra implementation](https://github.com/apache/cassandra-accord)
* [DistSys reading group presentation](https://www.youtube.com/watch?v=xeQHa3Z-d-A)
* [Cassandra on ACID presentation](https://www.youtube.com/watch?v=7Nm8mEcKrRc)

Properties:

* Leaderless consensus (use any quorum).
  * Avoids long WAN jumps to remote leader.
  * Avoids unavailability following leader loss.
* Strict serializable cross-shard transactions.
* 1 RTT fast path (no contention), 2 RTT slow path (contention or clock skew).
* Can reorder commutative operations (e.g. reads).
* Fast path requires ~3/4 supermajority.
* Fast path requires moderate clock sync (e.g. NTP).
* Requires known read/write set (no interactive transactions).
* Requires quorum reads.
  * Can it be extended with e.g. read leases or closed timestamps?

