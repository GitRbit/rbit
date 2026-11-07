# Bounded Wiki startup scratch ledger

Source prepared; every new Rust/native fixture is UNRUN. Migration0070 is reserved
for this atom and requires0069, which itself requires identity0068 and Wiki0066.
It does not replace or rewrite those migrations and enables no runtime/user routes.

The ledger consumes the actual stopped `FencedWikiRecovery` controller. Private FULL
connection authority, physical root/catalog retention and current preparing epoch
are required for every operation. Default/old connections cannot edit scratch rows.
It cannot be reconstructed from a persisted scan phase, parsed OID or copied marker.
The same trusted static coordinator must drive the actual native directory cursor
and persist each nonempty native record batch before consuming its continuation.
Record enums are data, not caller-authorized native/head receipts.

The explicit policy limits total scopes, unique objects, edges and charged logical
record bytes. A scope is64 logical bytes, an object128 and an edge128, covering all
its fixed stored metadata; no document body is copied into scratch. Counts/charges
and a maximum256-record batch commit atomically. Capacity, malformed records,
duplicates and commit errors do not silently increase the budget. SQLite indexes,
pages, WAL and filesystem allocation are NOT included in these logical charges.
Actual filesystem headroom/physical-size admission remains a mandatory closed
coordinator gate; these counters alone are not a physical-byte reservation.

Unique keys include current owner epoch, nonreused WikiId and SHA-1 OID. Scope
creation checks actual catalog/pending identity by indexed point queries; SQL
orphans keep their original WikiId. Objects start incomplete. Ordered tree/commit
edge facts and commit attribution are validated before an explicit Complete record
can mark that object complete. Repeated page IDs and skipped/replayed ordinals
refuse. Scope EOF is an idempotent data marker only after all objects are complete;
SQL cannot independently establish that an actual native iterator reached EOF.

Once all claimed scopes end, keyset batches join each edge to its source and target
object rows by their primary keys. Missing/incomplete/wrong-kind targets refuse
without promoting the phase. An opaque in-process cursor is tied to the actual
ledger owner; foreign/stale continuations refuse. Reaching the end advances only
to `agreement`, never ready/managed/native-head/durable publication. Historical SQL
parent/generation/page agreement, exact accepted selected-blob bytes, root/ref
closure and platform synchronization still must follow in the closed bundle.

Old scratch rows are never adopted across owners/restarts. Before a new scan,
cleanup removes at most the selected number of edges, objects and scopes each.
The candidate object/scope key range is bounded BEFORE checking child eligibility,
so blocked prefixes cannot cause a full-history eligibility scan. There are no
delete cascades, unbounded DELETEs or hidden VACUUM. Only after all three tables are
empty are quota counters reset. A failed new-scan attempt requires cleanup through
a newly acquired stopped controller; no scratch state is silently cleared.

Uncertain record-batch ACKs do not authorize cursor advancement. This intermediate
coordinator contract restarts that pass under a fresh owner/epoch and bounded
cleanup rather than guessing which native records committed. A disk-full or lost
owner similarly pauses startup without a head receipt. Native-only orphan-root
enumeration and global enabled/pending scope admission must finish before readiness;
the caller cannot omit scopes and use `agreement` as a completeness proof.

Prepared real-store tests cover bounded ordered joins, unique records, incomplete
and wrong-kind/missing edges, atomic object/edge/logical-byte limits, page/ordinal
rules, old-owner cleanup/rescan, foreign cursors, stale epochs and default-writer
refusal. Root must run `-p rbit-storage --lib sqlite::wiki_mutations::full::scratch`,
all Wiki/FULL/protocol tests, supported-platform tests and strict Clippy/AST after
the entire prerequisite chain is composed.

The explicit service `WikiStartupCatalog` now composes the actual native cursor and
owned SQL ledger. It persists each batch before installing the next continuation,
sets a sticky poison bit before its first await, and refuses reuse after error or
caller cancellation. Native EOF is acknowledged only after the SQL scope marker
commits. Actual empty-scope absence does not initialize a Git repository; it remains
data for later exact catalog agreement. Real native-to-SQL fixtures prepare accepted
objects, require three hashes/nine records, exercise empty scopes, and refuse corrupt
native bytes or rejected work without an EOF/visible pointer. These fixtures are
also UNRUN; synthetic SQL record fixtures alone are not native proof. Run services
`--test integration wiki_reads::startup` with the complete prerequisite chain.
