# Owned FULL-synchronous SQL acceptance

Status: source prepared; Rust execution, strict Clippy and formal AST checks pending.
This adds an explicit storage write path, not central Services/network activation,
Git durability, a validated prior head, visible publication or W2 completion.

## Actual connection and ownership

`FullWikiWriter::open` consumes the noncloneable actual catalog lease. It clones the
real pool's connection options, preserves writer compatibility and existing tuning,
disables creating a missing database and changes only synchronization to FULL. Its
dedicated pool has at most one connection (no minimum, idle expiry or lifetime expiry).
Every physical connection, including accounted replacements,
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

One supervisor is installed before the first async physical connection setup. Losing
an opening result or the last controller signals this same supervisor; it holds the
actual catalog, not another controller. Embedders explicitly call `close` with a live
runtime: close admission, wait for actual command-future destruction, explicitly
detach/close the sole physical connection and await its result, then close the pool.
Optimize-on-close is disabled so unrelated maintenance cannot precede native shutdown.
Cancelling a drain waiter does not relinquish cleanup ownership. Escaped accepted/native
values can still intentionally retain catalog exclusion after a verified close.

`SqlitePool::close` alone is NOT native shutdown proof: pinned SQLx 0.9 ignores raw
close errors and has discard paths using `close_hard`. The adapter counts physical
setup generations and only accounts a generation after explicit native close succeeds.
An initialization guard marks cancellation/error before after_connect as unknown.
Unexpected replacement, failed setup/close, overflow, runtime/supervisor abort or
unaccounted generation cannot release the retention guard or mint a positive receipt.
Ordinary policy/constraint rejection does not poison a healthy writer.

On unproved Drop, the private guard intentionally retains exactly one actual catalog
lease with `mem::forget`; it does not run a reaper loop or enter a global registry.
`Quarantined` is distinct from successful close. Process restart is required to recover
this retained resource. The exclusive subject/adjacent lock prevents repeated owners
for the same physical catalog; host composition must bound configured catalogs. This
conservative failure boundary also survives runtime abortion, rather than depending on
a runtime task remaining scheduled forever. No automatic repair or all-error release
is claimed. Commit I/O failures remain uncertain SQL outcomes to reconcile.

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

Closed temporary-store fixtures cover FULL on initial/accounted-replacement connections
while the general pool remains NORMAL; committed but invisible acceptance; receipt
lifetime; deferred-FK final-COMMIT failure with all allocations rolled back;
unpolled response cancellation plus cancelled/retried drain; busy/closed admission
without a job; unknown-origin replay refusal without restamping; and exact new-command
SQL work parity with the legacy worker. Additional ownership cases cancel setup at an
actual established-connection boundary and discard a successful opening result while
a real connection is borrowed. Controlled subprocess faults abort a runtime or cause
an unaccounted replacement; repeated acquisition must remain Busy. Parent-owned temp
directories are removed only after each child exits, avoiding leaked test-process
handles. The 30-second child safety deadline is not a native-termination inference.
These are source-prepared tests, not actual
power-loss simulations. Run storage `--lib wiki_mutations::full`, then prior Wiki
acceptance/ownership suites and strict Clippy/AST through the shared build helper.

The only new runtime edge is storage to the already-workspace-pinned tokio-util
TaskTracker; no new registry package/version or workspace member is introduced.
