# W1 wiki catalog and immutable object read foundation

Status: source prepared, uncompiled. This atom follows W0 and adds real SQLite/native
Git adapters plus the feature-owned read application. There is no production wiki
publisher, route/UI, enable command, write service, recovery worker or memory backend.
The full W0-W5 plan remains mandatory, including publication/recovery and polished UI.

## Prerequisites and authority

Migration 0065 MUST follow accepted 0063 allocator and 0064 security recovery migrations.
Its first temporary constraint checks successful versions 63/64 in the SQLx migration
ledger; without both it fails before creating wiki state. Root integration must apply
all source prerequisites in order before compiling/running migration-bearing packages.
This is an ordering guard, not a cryptographic upgrade attestation. No old migration
is changed. No live user data was opened or migrated during this source preparation.

SQLite owns one persistent random 16-byte WikiStoreId, enabled repository catalogs,
nullable never-initialized head, positive global generation/full object ID, stable
AUTOINCREMENT page IDs, canonical case-sensitive paths/titles and bounded-query revision
history. Composite keys/FKs bind page history to its own repository and revision.
Revision/history composite tables use WITHOUT ROWID; page allocation retains row IDs.

The native object root's fixed 16-byte marker must match that catalog identity. It is
a namespace/configuration binding, not an authenticated realm, ACL, secret or publication
receipt. Paired backups intentionally preserve it. No user may supply a store root,
marker, selected page record or admin flag over a network to bypass composition.

Catalog heads reference immutable finalized revision rows. Page reads use only that
SQLite-visible generation/OID, never raw Git HEAD. Historical reads select a recorded
page generation at or below that head; arbitrary OID reads are not a feature API.
Stored metadata must already be canonical: decoding consumes the existing String and
rejects normalization/trimming changes instead of allocating another copy or quietly
changing the namespace key. Source text remains untrusted Markdown, not safe HTML.

## Current authorization and bounded read work

WikiCatalogStore uses the existing shared readable_by SQL predicate, with current
administrator/suspension facts resolved inside the same SQLite read transaction.
A denied/missing/disabled repository returns before page/history metadata fetching.
A caller supplies only an authenticated account ID or an anonymous visitor; no cached
admin boolean is trusted. This slice does not implement task/deploy principals or PAT
scope caps: future API adapters must enforce scopes before using the user read port,
and must not coerce another principal into an unrestricted account ID.

This is snapshot authorization at the read's linearization point, not a promise to
cancel every in-flight response after a subsequent ACL revocation. Subsequent requests
resolve access again. No conditional 304, cache or HTTP response path exists yet.

An empty enabled wiki or denied read takes one acquisition/statement and no Git work.
A current or recorded page takes one acquisition/two statements. History takes one
acquisition/three statements, at most 51 SQL rows and 50 returned entries, with no per-row
SQL. Cursor bounds become an indexed inclusive generation range, avoiding deep nullable
OR scans and preserving i64::MAX. Missing/future targets do not admit a Git reader.
These are prepared count_work assertions, not executed results or latency measurements.

WikiReader checks returned repository/page/head consistency before invoking the native
port. Only finalized selected content is returned. Empty catalogs and denied access
skip object admission even if a prepared commit exists or raw HEAD points at it.

## Immutable native object preparation

WikiObjects uses a dedicated root and positive numeric repository/page paths, never
owner names, user wiki paths, code refs or code storage layouts. An existing nonempty
unmarked directory cannot be adopted. Root markers, observed symlinks/reparse points,
critical Git paths and alternate object databases are checked/refused. Supported use
requires an owned local filesystem and stable topology; this is NOT protection against
hostile same-user filesystem replacement, topology ABA, hardlink tricks or network
filesystem semantics. Normalized logical paths are never joined into OS storage paths.

Reads/writes use pinned gitoxide 0.90 native APIs with isolated repository options,
without shell commands, Git child processes, hooks or workflow execution. Current
workspace Git features are SHA1-only; pure W0 identity supports SHA256 syntax, but
this adapter does not claim native SHA256 support and refuses unsupported IDs.

Explicit BlockingBudget injection bounds admitted native work; there is no numeric
production default or hidden queue. A worker retains its permit/root ownership after
an awaiting request is dropped. Composition must own close/drain and object-root
teardown; no false timeout/stopped proof or integration with a host is claimed here.

Object headers bound commit 64 KiB, tree 1 MiB and content 256 KiB before body lookup. Flat
trees hold at most 10,000 canonical positive numeric .md blob entries. A streaming pass
rejects duplicates, noncanonical ordering/names and other modes with no decoded-entry
vector/per-entry formatted String. Preparation uses one mutable entry vector and
binary-search insertion/update; page body and raw OIDs are not copied into another DTO
or hex String. Actual costs, object-cache behavior and limits still require measurement.

prepare writes immutable blob/tree/commit objects and returns a syntactic OID. It
DOES NOT move a ref, make the object GC-reachable, establish object/fsync durability,
commit an intent/catalog mutation, or prove successful recoverable publication. There
is no automatic GC/maintenance worker for this root. Do not run GC against prepared
or SQL-selected objects until the future publisher/backup protocol pins all required
roots. Orphan cleanup and disk quotas remain required before accepting network writes.

## Exact remaining runtime gap

The NEXT publisher must jointly implement current write/archive/namespace authority,
stable ID allocation/tombstone/incarnation rules, finite preparation/storage quotas,
fixed durable intents, expected-to-target Git ref CAS, object/ref durability and pinning,
exact same-intent SQL finalization, lost-acknowledgment/crash/cancellation recovery and
owned shutdown. A bare returned OID or caller assertion is not a receipt to publish.
No arbitrary-OID publisher API is provided to disguise those missing guarantees.

Aliases/tombstones are not implemented in this schema. Whole-wiki deletion must never
reset optimistic state or erase cleanup evidence; repository deletion currently cascades
read catalog rows and has no wiki object cleanup journal. Lifecycle integration is a
mandatory subsequent gate, not safe production deletion behavior. Backup/restore,
permissions/scopes, media/search/cache, startup/admin enable and migration/upgrade
acceptance must also be completed. HTTP/Askama read UI follows only a coherent actual
runtime composition; broad editing UI must wait for the tested publication protocol.

## Prepared validation

Closed temporary fixtures seed finalized SQL rows ONLY for store tests, with actual
native commit objects. This is not a production seeding API or local dogfooding bypass.
Five services fixtures cover empty/unpublished HEAD, committed/history reads, private
ACL grant/revocation/suspension/disabled denial, archive/canonical corruption and maximum
generation pagination. Native fixtures cover unpublished preparation/old revisions,
admission/store binding, unmarked/foreign marker preservation, malformed duplicate/
unsorted trees, body limits/UTF8/NUL and Unix linked roots. Domain tests add content,
canonical decoding and history/store bounds. None of these Rust fixtures ran here.

Required integration: register W0 member/alias, minimal lock edges, accepted 63+64 then 65;
run rbit-wikis tests, Git integration wiki_objects, services integration wiki_reads,
strict all-target Clippy/AST and unified graph checks, full storage upgrade/deletion
coverage and full workspace. Additional path/namespace/crash/footprint/performance
fixtures remain gates; a source scan is not their substitute. No Cargo metadata/tree,
compiler, native runtime or shared cache was used during this source-only preparation.
