# Closed wiki writer, replay and startup contract

Status: proposed next W2 contract; no migration/FFI/runtime activation in this atom.
Prepared ownership/FULL acceptance remain intermediate. SQL 0069 is reserved for
wiki protocol/scan state after identity 0068; immutable 0065/0066 are not rewritten.

## Software fence and deletion compatibility

0069 introduces one singleton protocol state: legacy, preparing, managed. Default
legacy preserves existing inactive read/preparation fixtures and NORMAL acceptance.
Transition requires the actual catalog-adjacent lease, managed root lease and an
owned FULL connection. Preparing/managed reject default/legacy writes to every
0065/0066 table, including protocol control, actor/page allocation and intents.
Persistent BEFORE INSERT/UPDATE/DELETE triggers require a distinct wiki-writer
collation registered ONLY on the private owned FULL connection. It is a software
compatibility capability, not an authentication secret or a parsed durable proof.

Cached legacy statements must refuse after transition through SQLite schema
repreparation. Test a statement executed/cached before activation, not merely a new
connection missing the collation. Read-only queries and unrelated request SQL retain
their existing shape and GENERAL NORMAL pool synchronization.

This is a closed upgrade: drain already-open old hosts before activation. Persisted
schema/version preflight rejects an incompatible new startup before background work.
SQL triggers cannot restrain arbitrary old same-user filesystem writers ignoring a
root lease; the supported deployment excludes them. Existing W1 preparers must honor
the root lease and cannot mint managed readiness from a marker or public constructor.

No DELETE exemption is silently added. Guarding wiki FK children also affects normal
repository/account deletion. Those workflows must enter an actual owned wiki
destructive-operation bridge BEFORE disk/network side effects, close appropriate
write admission, drain real native work and retain owner/epoch identity through the
SQL cascade. Account deletion must also respect identity's independent ACTIVE writer
protocol. Do not globally register Wiki authority on a general pool to evade this.
An old disk-first deletion must never discover a new Wiki refusal only after removing
code data. These bridges are activation prerequisites, with real delete/fault tests.

Enable/disable, transfer, backup/restore and GC similarly use explicit static owner
ports, not a ServiceContext locator. Deleting a repository may leave orphan intent
evidence; no cascade or cleanup erases pending/quarantined payload/identity. Such
orphans continue consuming the configured pending/proof-entry budget until a real
terminal recovery proof permits release. Read privacy/ACL-before-Git stays unchanged.

## Replay synchronization

The shared SQL worker explicitly distinguishes New from Replay. New own-FULL COMMIT
may acknowledge conditional SQL acceptance. Replay's read-only COMMIT cannot do so.
Do not restamp its time, allocate another page, replace its body or silently freshen
its expected global head. Preserve the original request identity after uncertain ACKs.

After a supported writer fence exists, run replay synchronization on the owned FULL
connection, outside a transaction, under the same catalog/root ownership:

1. Load exact pending intent and fresh actor incarnation/permission/archive facts
   using the existing acceptance checks. Completed replay remains a separate exact
   historical-body/native-closure path, never merely metadata success.
2. Execute `PRAGMA main.wal_checkpoint(FULL)` with the existing bounded busy handler.
   Require busy=0, nonnegative total/checkpointed frame counts, and equality. Unknown
   journal mode, errors, busy or partial completion yield Pending/uncertainty.
3. Only then produce an owner-bound SQL acceptance fact for that exact stored command.
   No checked bool or caller OID enters a native/visible publication constructor.

SQLite documents FULL as waiting for current writers/readers, checkpointing all log
frames and syncing the database ([contract](https://www.sqlite.org/c3ref/wal_checkpoint_v2.html)).
The flush is conditional on VFS/filesystem/device guarantees; not a power-loss test.
Costs are replay-only. New acceptance retains its existing statement/acquisition
budget; ordinary reads gain no checkpoint, metadata query or hidden connection.

Tests must exercise an actual NORMAL-origin pending row, exact retry after lost
FULL ACK, an older held reader causing an incomplete checkpoint (private test busy
handler, no scheduling sleeps), successful retry after releasing it, unchanged
request metadata, and no positive result on foreign mode/failed checkpoint.

## Bounded resumable startup

Startup holds actual DB-derived and native root ownership; no wiki writes become
ready until protocol, catalog identity, object integrity and platform fence agree.
Use private same-owner cursors resumed between bounded work batches, not all-history
Vectors/HashSets or a full scan inside each edit. A process restart does not trust
a saved cursor/marker as a proof of bytes: rebuild validation under the new owner.

Enumerate all enabled scopes and pending/quarantined/orphan scopes with indexed
keyset SQL. Reserve one private map entry per distinct scope before scanning/enable;
an explicit maximum covers all such entries. No LRU eviction, silent proof loss or
unbounded new scope adoption. Exhaustion pauses write readiness with a typed reason;
it does not quietly classify valid stored data as corrupt or weaken read ACLs.

Each batch has explicit object/byte/SQL-row/time budgets and an owned blocking
reservation. Retain the actual directory iterator/cursor and resources between
batches. Each loose object is decoded, size/kind checked and hashed once per startup
pass; names/paths, canonical tree uniqueness/order, commit shape and all referenced
objects must agree. Verify historical SQL generation/parent order, stable page
identities, selected page metadata, current SQL roots and live private refs. Pending
targets additionally compare the selected blob bytes EXACTLY to accepted SQL body,
not only its Git hash. Packed/foreign/alternate/unrecognized stores quarantine this
intermediate implementation, with pack/backup/GC support still mandatory later.

Exact reachability/orphan classification cannot use an unbounded in-memory OID set.
Proposed 0069 startup ledger stores scan epochs/object-kind/reachable/edge work in
indexed SQLite batches, bound to store/wiki/incarnation and held owner epoch. It is
scratch/recovery data, never persistent proof. Bound RAM by batch size and cap disk
growth explicitly; pause/resume on storage pressure. Cross-process restart rechecks
previous bytes. Ledger cleanup is indexed/batched after proof invalidation, not a
surprise full DELETE/VACUUM on a page request. This design needs startup I/O/RTO/space
measurement before adoption; external-sort alternatives can replace the ledger if
they preserve the same bounded exact closure contract. No registry dependency needed.

No all-history scan is added per edit. After startup, the private owner map holds a
validated head and checked invalidation epoch, not a copy of all document bodies.
Only the native owner may mint a prior-head capability after actual platform fencing.
Caps retain the owner/scope, match its store/wiki/head/epoch and cannot be deserialized.
Persistent markers and SQL IDs are data, not such capabilities.

## Incremental publication and uncertain phases

Normal edits extend the verified immutable closure with new blob/tree/commit only,
plus required namespace/ref bindings. Reuse the old closure through the owner-scoped
capability; no whole-history/current-tree blob rehash. Existing-object equality is
exact kind+wire bytes and never a gix replacement shortcut. New target proof also
binds original accepted page/path/title/actor-incarnation/time/body/expected head.

Bind target T once to the accepted SQL intent using a FULL transaction BEFORE any
native ref transition. Native expected-ref CAS H→T and required platform flush occur
before SQL visible pointer/history/page rows can commit. Fresh write ACL, archive,
incarnation, store/wiki and expected head are rechecked in that final transaction.
The owner map remains Pending(H,T,command) until the actual same-intent visible
commit acknowledgement; no early success based on ref movement alone.

Ref T with pending SQL after a crash is not automatically corruption or success.
Reconcile exact fixed intent, closure/body, actual ref and fresh policy. Lost SQL ACK
requires reading the actual same-command visible state before rollback assumptions.
Unavailable/foreign/drifted state remains quarantined and counted. Ref H can retry
the deterministic T or prove a terminal abort. Permission denial after T requires a
verified durable T→H rollback (or durable absence for initial H=None) before terminal
Aborted. Arbitrary conflicting refs never get silently overwritten.

Only after actual durable visible completion may transient SQL body bytes clear.
Completed replay verifies exact historical native body plus fixed command metadata.
Missing/corrupt visible closure cannot be repaired from a new caller body. Successful
native preparation, SQL acceptance and visible ACK are distinct typed phases.

GC/restore close admission, drain actual workers, then advance a checked invalidation
epoch BEFORE touching storage. Old caps must refuse even if head IDs happen to match.
An overflow permanently closes admission. Resume requires fresh bounded validation
and platform fencing; no stale cap survives copying/restoring matching markers.

## Concrete acceptance gates

- Raw/default/cached old INSERT/UPDATE/DELETE all refuse after fence; reads and
  unrelated workloads retain budgets. Real account/repository delete bridges keep
  side-effect/error ordering and avoid disk-first partial loss on admission refusal.
- Separate compositions/root copies of the SAME actual DB cannot bypass leases;
  hardlink/unsupported aliases refuse; scope/orphan budgets cannot grow or evict.
- Checkpoint busy/error/partial frames never acknowledge replay; exact retry succeeds
  only after real flush and unchanged identity/policy. No generic auth proof implied.
- Interrupted startup resumes within the same held owner; restart revalidates bytes.
  Missing child, reordered/duplicate tree, foreign marker/ref, hidden pack and exact
  payload mismatch quarantine. Large valid history meets bounded RAM/startup RTO.
- Phase fault matrix covers target-bind/ref-CAS/flush/visible-COMMIT/ACK/rollback gaps,
  CAS losers, account/repository reuse and cancellation of the actual owned worker.
- Per-edit traces prove three new-object work at most, no historical blob scan or
  extra read-path SQL. Full Windows/Unix native durability gates precede activation.
