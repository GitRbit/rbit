# Bounded Wiki startup scope enumeration

This is the first SQL enumeration stage of W2 startup, not an integrity receipt,
ready state, historical closure validator or usable publisher. It extends the
stopped `FencedWikiRecovery` owner; it does not enable routes or alter page reads.
The prerequisite identity migration 0068 and Wiki fence 0069 must be composed by
the integration owner before running the complete-schema fixtures.

`startup_batch` takes an explicit size between 1 and 256. One snapshot checks the
actual preparing epoch, reads a primary-key catalog range and reads the existing
partial active-intent index. Each source fetches at most size+1 rows; a sorted merge
produces at most size projections. There is no full-catalog UNION/sort, per-scope
query or unbounded in-memory object set. The revision projection has a byte bound
before Rust decoding. Invalid heads and cross-source identity disagreements refuse
the batch rather than manufacture a head or adopt another repository.

Disabled catalogs and pending/quarantined SQL orphans are included. A deleted
repository's pending WikiId remains separate from a new wiki belonging to a reused
numeric RepoId. These projections confer no read/write access: startup is trusted
stopped composition, and every later user operation still needs fresh authorization.

The continuation retains the actual writer and object permission guard. It is
bound to their in-process identity and preparing epoch, cannot be deserialized or
constructed externally, and is rejected by another owner or after an epoch change.
Calling the synchronous admission method adopts its SQL job before returning the
future. Cancellation of the response does not release the root permission while
that job is running. A process restart must start validation again; a saved numeric
WikiId is not a validated continuation or historical closure capability.

Prepared tests cover two-source deduplication, bounded resume, disabled scopes,
orphan/reused repository identity, foreign/stale cursors, malformed projections and
three statements/one acquisition per batch with indexed range plans. Rust/native
execution is UNRUN for this atom. Node formatting/source checks cannot establish
SQLite behavior. Integration acceptance includes the storage `full::startup` unit
tests, all Wiki/FULL/protocol fixtures, strict Clippy and the cumulative AST gate.

Remaining mandatory startup work includes native-only orphan enumeration, bounded
and resumable current/historical object validation, platform synchronization,
restore/pack/version policy, and the explicit enabled-wiki plus orphan budget.
Only their closed protocol may mint owner/epoch-scoped prior-head capabilities;
this enumerator cannot. The incremental edit path must then validate/synchronize
only its new blob/tree/commit rather than rescan historical content on every edit.
Destructive repository/account bridges still must compose the actual identity and
Wiki owners before any disk effect, never fabricate identity callbacks from copied
connection options. No W2 completion or whole-process stopped guarantee is claimed.
