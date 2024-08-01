# rust-accord

Aspirational Rust prototype of the Accord consensus protocol.

* [Accord paper](https://cwiki.apache.org/confluence/download/attachments/188744725/Accord.pdf)
* [Cassandra implementation](https://github.com/apache/cassandra-accord)
* [DistSys reading group presentation](https://www.youtube.com/watch?v=xeQHa3Z-d-A)
* [Cassandra on ACID presentation](https://www.youtube.com/watch?v=7Nm8mEcKrRc)

## Properties

* Leaderless consensus (use any quorum).
  * Avoids leader bottleneck.
  * Avoids long WAN jumps to remote leader.
  * Avoids unavailability following leader loss.
  * Avoids slow leader on critical path (e.g. disk stall or packet loss).
    * However, client ↔ coordinator is on critical path.
* Strict serializable cross-shard transactions.
* 1 RTT fast path (no contention), 2 RTT slow path (contention or clock skew).
* Fast path requires ~3/4 supermajority.
  * Does it? Flexible fast-path quorums can be stable under f failures, works with 2f+1.
  * On failure, must reconfigure fast path quorum to be equivalent to slow-path quorum.
    * Transient slow-path latency blips until fast-path quorum is reconfigured.
* Fast path requires moderate clock sync (e.g. NTP).
  * All txns must wait out clock skew interval. Skew >> RTT on LAN, fall back to Paxos?
* Can reorder commutative operations (e.g. reads).
* Requires known read/write set (no interactive transactions).
* Requires quorum reads.
  * Median latency, rather than optimal latency.
  * However, implicitly provides hedged reads.
  * Can it be extended with e.g. read leases or closed timestamps? Should it?
  * Alternatively, use inconsistent reads or most recent local replica.
