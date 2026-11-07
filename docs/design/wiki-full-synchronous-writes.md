# Owned FULL-synchronous SQL acceptance

Status: source prepared; Rust execution, strict Clippy and formal AST checks pending.
This adds an explicit storage write path, not central Services/network activation,
Git durability, a validated prior head, visible publication or W2 completion.

## Actual connection and ownership

`FullWikiWriter::open` consumes the noncloneable actual catalog lease. It clones the
real pool's connection options, preserves writer compatibility and existing tuning,
disables creating a missing database and changes only synchronization to FULL. Its
dedicated pool has one connection. Every physical connection, including replacements,
must read `PRAGMA synchronous` as 2 during setup before it enters the pool. Setup
is not a per-read/request query and does not mutate general NORMAL-pool settings.

The open operation itself runs as an owned startup task. If its waiter is dropped,
the actual catalog resource remains with connection setup until the real future and
its result are destroyed. There is no public raw connection/options accessor and no
constructor accepting an arbitrary claimed database path. A narrow cloneable
`WikiCatalogHandle` keeps the actual lease for native binding without keeping a
writer controller alive or permitting another dedicated pool from the same lease.

Admission is synchronous and nonqueued before any SQL pool acquisition. The single
admitted command is adopted by a TaskTracker under the same synchronous gate as
close. Its task captures only the real pool, catalog handle and admission permit;
it does not retain its controller/tracker. Dropping even an unpolled response future
does not cancel that task. The exact command identity must be retained by callers
after a cancelled/uncertain response. No mutex guard spans an await.

Embedders explicitly call `close` with a live runtime: close admission, wait for
actual command-future destruction, then await dedicated pool closure. Cancelling a
drain waiter does not relinquish task ownership. Escaped accepted/native values can
still intentionally retain catalog exclusion. Runtime teardown/abort, panic and
commit I/O failures are not successful receipt paths; they require reconciliation,
not a claim that all possible SQLite worker cleanup was proved by a dropped future.

## One SQL worker, provenance-aware result

The existing acceptance implementation is shared through private `accept_on`. Its
SQL strings, authorization/replay/archive order, transaction and normal work budget
are unchanged. Only the private return records whether it inserted a new command or
found an exact pending replay. The legacy `WikiMutationStore::accept` preserves its
API/semantics and remains honestly NORMAL; it is not a power-loss durability API.

A new command yields `CommittedWikiAcceptance` only after its actual FULL-connection
transaction committed. The noncloneable result keeps both the exact intent and real
catalog resource; it has no public field constructor. This acknowledges SQL only,
conditional on SQLite/VFS/filesystem/device synchronization behavior, not a portable
hardware theorem or current authentication evidence. It does not authorize native
ref publication, SQL visible pointers, or a caller-supplied OID.

An exact replay commits a read-only transaction. FULL on that connection does not
retroactively synchronize earlier NORMAL-origin WAL frames. Such a replay returns
`ReplayRequiresFence`, never a positive receipt, and keeps its original page,
generation, time, actor incarnation and body unchanged. This includes repeats of a
FULL-origin command after its caller lost the response: private provenance is not
guessed from persistent data or a parsed marker.

Before positive replay receipts, the closed publisher must establish the supported
same-protocol wiki-writer fence and run a replay/startup synchronization operation
under the actual owner. The concrete candidate is `PRAGMA main.wal_checkpoint(FULL)`
on the dedicated FULL connection, outside a transaction: require busy=0 and
nonnegative equal total/checkpointed frame counts; busy/error/incomplete/unknown mode
preserves the pending command and yields uncertainty. SQLite documents FULL as
waiting for current readers/writers, checkpointing all frames and syncing the
database ([checkpoint contract](https://www.sqlite.org/c3ref/wal_checkpoint_v2.html)).
This replay fence is deliberately not implemented/minted before the writer-version
exclusion contract exists. It must not silently restamp or resubmit the command.

The existing catalog lease excludes cooperating new owners but cannot stop an old
binary/API that ignores it. Closed upgrade/version exclusion, startup integrity,
restore/GC epochs and platform namespace coverage remain mandatory before routes.
No global SQLite performance setting changes in this atom.

## Prepared acceptance evidence

Seven closed temporary-store fixtures cover FULL on initial/replacement connections
while the general pool remains NORMAL; committed but invisible acceptance; receipt
lifetime; deferred-FK final-COMMIT failure with all allocations rolled back;
unpolled response cancellation plus cancelled/retried drain; busy/closed admission
without a job; unknown-origin replay refusal without restamping; and exact new-command
SQL work parity with the legacy worker. These are source-prepared tests, not actual
power-loss simulations. Run storage `--lib wiki_mutations::full`, then prior Wiki
acceptance/ownership suites and strict Clippy/AST through the shared build helper.

The only new runtime edge is storage to the already-workspace-pinned tokio-util
TaskTracker; no new registry package/version or workspace member is introduced.
