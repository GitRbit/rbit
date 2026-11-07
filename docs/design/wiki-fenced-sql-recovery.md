# Stopped Wiki fence and exact SQL replay

Status: SQL/storage Rust suites run; rbit-platform native fixtures run locally,
unsafe audit open. 0069 MUST follow
identity0068 and Wiki0066; do not apply it independently or rewrite previous
migration versions.
This adds real storage/recovery operations but no central/network activation,
Managed transition, startup-integrity completion, native-head or visible capability.

## Actual owners, private permission

`FullWikiWriter::begin_recovery` consumes its startup owner and an actual
`WikiObjectOwnership` I/O port. Production's implementation is the existing native
`CatalogBoundWikiObjects`, which holds the real root lock and catalog resource.
Inside admitted blocking work the factory checks both resources, exact store ID
and the ACTUAL canonical catalog path. A foreign actual catalog/root refuses before
any protocol update. This trusted I/O composition is not authentication or a
cryptographic attestation from arbitrary embedded Rust implementations.

The initial FULL connection starts with only immutable legacy Wiki permission.
After those checks its actual worker is physically closed/accounted; a second FULL
generation then registers an immutable PRIVATE comparator retaining the actual
object resource. Neither registered comparison changes its answers during its
lifetime. Jobs and accepted results hold that same root guard. A typed retention
guard is installed before the handoff task's first await and transfers into the
replacement pool's actual shutdown supervisor before physical initialization.
Only proved physical close releases it; uncertainty/runtime abort retains both the
actual root and catalog until process restart. No caller boolean, parsed ID,
exported options function or marker grants this role.
Fresh standalone FULL owners of a preparing/managed database therefore do not
inherit closed Wiki permission merely by opening the catalog.

The one synchronous gate/try-admission path covers legacy acceptance, fence setup
and exact recovery. Claiming a protocol owner disables old FULL clones' unfenced
entry point. Jobs are tracked before returning even an unpolled response future;
cancellation retains actual root/catalog resources. This remains bounded work with
no pre-pool wait queue. Startup adds one actual LEGACY close and one PRIVATE physical
open/PRAGMA, never simultaneous active generations. The handoff waits outside its
tracked validation job to avoid self-drain deadlock. A discarded response cannot
cancel the handoff. Old controller clones remain permanently claimed and closed;
a new standalone FULL owner always starts with only the stable LEGACY callback.

## Software compatibility and phase

0069 has legacy/preparing/managed control with a nondecreasing nonnegative epoch.
Preparing entry advances the epoch with checked arithmetic in a FULL transaction.
The control row cannot be deleted or downgraded to legacy. This slice can only enter
Preparing; it cannot declare Managed or Ready. Namespace identity becomes immutable
once the fence is preparing/managed, even for the private connection.

All 0065/0066 INSERT/UPDATE/DELETE paths require the Wiki collation. Current general
connections register a well-formed legacy equivalence-class comparison; it cannot
write preparing/managed. The private callback uses a different well-formed comparison
ONLY while its actual root-resource guard is held. No nonreflexive ordering is used.
An absent control row fails closed. The callback is stable during each admitted SQL
operation because that operation retains the guard and the connection is serialized.

Previously cached registered-default statements observe the new mode row and deny.
Unregistered old connections cannot resolve the new collation after schema
repreparation and refuse already after DDL. Reads and unrelated general statements
are unchanged. Legacy acceptance's nonmutating data/replay paths are not proof of
managed permission or a positive receipt; actual writes are guarded by SQLite.

Fences also protect FK-cascade children. Therefore actual repository/account delete
and other destructive bridges MUST enter owned Wiki admission before existing
disk/network side effects. They must combine the separate identity ACTIVE writer
protocol appropriately. Do not enable this mode in a live host until those bridges
are implemented/tested. No broad default-pool authority or DELETE exemption is added
as a workaround. Preparing is explicitly stopped maintenance/startup in this API.

## Replay really synchronizes

Recovery calls the exact shared acceptance worker with ExistingAtEpoch. It checks
the preparing epoch inside its writer transaction, then preserves the existing
fresh-account/permission/archive/actor-incarnation and exact payload ordering.
Missing commands return MissingIntent BEFORE new page/actor allocation or insertion.
No new request is manufactured; changed body/metadata refuses; original time,
generation, page and target remain unchanged.

Only after its exact read-only replay transaction returns does the same owned FULL
path execute `PRAGMA main.wal_checkpoint(FULL)`. Busy must be0 and total/checkpointed
frames nonnegative and equal. Errors/incomplete frames yield no accepted result and
leave the exact pending command for retry. Actual root/catalog binding is checked
again before returning `FencedWikiAcceptance`, which retains the real permission
guard. It acknowledges conditional SQL acceptance only, not native preparation,
current authentication, historical body integrity, or visible publication.

Completed historical replay still returns VisibleReplay until the actual native
exact-body/closure protocol is implemented. No arbitrary caller OID reaches a
publisher. Close drains SQL; it does not call the whole Wiki platform stopped while
native roots/prepared values remain alive.

## Work and prepared evidence

Normal new acceptance keeps the original SQL strings and acquisition/statement
shape. A startup factory adds one small control transaction. Closed replay adds a
single epoch point-read in its existing transaction and one actual checkpoint
statement/acquisition. General pool setup registers one legacy callback per physical
connection; no per-page query/checkpoint/admission is added. Wiki DML triggers perform
singleton point reads/callbacks internally; outer statement parity is not a claim of
zero additional SQLite VM work. Measure write/cascade costs before adoption.

Prepared unit fixtures exercise real NORMAL-origin replay/checkpoint, an older real
reader preventing checkpoint completion then release/retry, exact no-restamp/missing
behavior, cached default update/insert/delete refusal, destructive FK-cascade refusal
without lost rows, stale epoch, no downgrade and cancelled response/drain ownership.
The SQL-only tests use a physical file-lock object-owner port; separate service tests
compose the actual native Git owner and reject a foreign actual catalog/root. A
same-connection minimal-schema mechanism fixture tests an old cached unregistered
statement against exact0069, independently of the complete-schema fixtures.

The stable-collation correction additionally prepares a real two-generation test:
the initial worker is accounted/closed before the PRIVATE owner appears; dropping
results never changes the PRIVATE comparator; fresh standalone FULL remains LEGACY.
A borrowed initial physical connection deterministically blocks the handoff while
its discarded response still retains the root; release permits replacement and
verified final cleanup. Controlled subprocesses exercise private-generation runtime
abort and unaccounted replacement, requiring both real catalog and root exclusion
to remain held; this test runs in the rbit-storage suite below.

SQLite requires a registered comparator to always return the same answer for the
same inputs; no-index usage does not waive that contract. The previous mutable
permission callback is removed rather than justified by the absence of indexes.
See the [official collation contract](https://sqlite.org/c3ref/create_collation.html).

Run on the shared build lane once identity 0067/0068 were published beneath 0069:
rustfmt, strict workspace Clippy, the source/AST checks, and the rbit-storage
Wiki/protocol/FULL suites, rbit-services `--test integration` (including
`wiki_reads::fenced_recovery`), rbit-git, rbit-wikis and rbit-identity suites.
rbit-platform is now a workspace member. Locally on Windows it builds, passes
strict all-target Clippy (also cross-checked for Linux) and passes its 6 package tests,
plus both explicit-scratch fixtures (cross-volume no-copy and loopback-UNC refusal).
See its [README](../../crates/rbit-platform/README.md#qualification-run). Still
open: a hosted CI result, a mapped-drive refusal run and an independent unsafe audit.
These suites are SQL/native-Git evidence, not power-loss proof. Next actual work remains
bounded startup catalog/native closure scanning and its resource budgets, incremental
epoch-scoped prior-head state, complete namespace bootstrap/ref durability, SQL
finalization/reconciliation, destructive bridges and user flows. W2 is not complete.
