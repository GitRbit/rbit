# Managed wiki object preparation boundary

Status: source prepared, Rust/native unrun. This follows the committed command
acceptance foundation. It provides real bounded native preparation and ownership,
not a ref publisher, durable prior-head capability, visible completion or W2 claim.
No route, enable command, host worker, schema finalizer or recovery loop is activated.

## Cooperating ownership

`WikiObjectWriter::open` holds a safe standard exclusive file lock for the dedicated
root. Clones share the actual held handle. Ordinary `WikiObjects::prepare` also takes
this lock around its real worker, so it cannot bypass an active managed owner. Reads
do not take it or gain new SQL/filesystem checks. Locks are nonqueued: contention is
Busy, unsupported/error is a failure, never permission to continue without ownership.

Managed preparation has a short synchronous per-wiki gate; no mutex crosses I/O or
an await. Its set is bounded by the injected blocking budget's capacity and refuses
both duplicate wiki work and excess prepared values without queuing. Preparation
returns a noncloneable value retaining this gate and root ownership while the future
coordinator persists its target. Dropping a waiter leaves those owners in the actual
queued/running blocking job until its result is destroyed. Prepared values do not
form an owner cycle; the set stores IDs, not tasks/values/controllers.

This is an explicit managed/cooperating-writers contract. External Git writers,
GC/repack, unknown legacy writers, hostile filesystem mutation, topology ABA and
network-filesystem guarantees are not covered. **Before activation**, a closed upgrade
must stop/drain older binaries and implement a version fence preventing them from
reopening for mutation. The new lock/old marker alone cannot establish absence of an
older worker that never honored the lock. No automatic format adoption is performed.

## Exact object reuse

Pinned gix 0.90.0 Repository write helpers check whether an ID exists and may reuse
it without comparing bytes. Their absent path delegates to gix-odb 0.87.0 loose
publication, whose tempfile `persist` can replace a destination. Thus ordinary
preexisting mismatch is silent reuse, while an intervening writer can create a
replacement race. API success is neither exact-content nor immutable-store evidence.

The shared wiki writer derives the expected native ID, then checks bounded header
kind/size and EXACT wire bytes. Existing equal objects are reused without a gix write;
different/corrupt objects fail with their incumbent bytes untouched. Absent objects
are written only under the held root/wiki ownership and are checked again afterward.
Both public preparation and private managed preparation use the same guarded path.

Blob input stays borrowed; it is not copied into a serialized-body Vec. Tree/commit
IDs are hashed through the deterministic pinned native encoder into a sink, and
existing bytes are compared through a writer over the loaded object. Only absent
objects use gix's existing single reusable wire buffer plus known-ID ODB write, with
no cloned entry vector and no second hash. Encoding/comparison and extra read costs
are unmeasured; no allocation-count or runtime-speed improvement is claimed.

Managed commit attribution includes the nonreused actor incarnation, not just the
reusable account ID. The exact command/metadata stays inside its private prepared
value. A persisted target mismatch fails; neither a syntactic OID nor constructing
WikiIntent can authorize publication or authenticate an actor.

## Required next publication work

The positive path must have exclusive, bounded/resumable startup/restore integrity
acceptance, then actual opaque owner-scoped validated prior-head capability. Per-edit
work must extend that managed immutable proof by verifying/syncing only new blob,
tree and commit plus necessary ref bindings. Scanning every page/history body on
each edit is not the production scalability solution. Parsed SQL heads or persistent
markers alone cannot mint a prior-head capability.

Fresh same-actor-incarnation authorization, fixed accepted payload/target comparison,
native expected-ref CAS, all platform durability operations, same-intent SQL visible
CAS, uncertain ACK/retry and verified rollback/quarantine remain required. Selected
page body comparison must be byte-exact even if its OID matches. Historical links may
resolve an actor only when its current incarnation still matches.

Windows remains unsupported for a positive durability receipt until a reviewed native
adapter owns every namespace binding with proven semantics. File flush after ordinary
gix rename does not close that gap. Safe local owner locks here are not durable-state
receipts. Other release platforms also need actual native/fixture qualification.

## Prepared tests

The existing native integration binary gains four tests: equal blob/tree/commit
mtime preservation; wrong-kind and same-kind/wrong-byte loose objects refused without
repair/overwrite; cloned owner/per-wiki/prepared capacity and lock lifetime; queued
request cancellation with real blocking-thread barrier and actual held-lock release.
These are prepared source, not executed results. Run `rbit-git --test integration
wiki_managed_objects` and all prior wiki object/service tests through the shared helper,
then strict Clippy/AST. Full publisher crash/replay/visibility and platform tests remain.
