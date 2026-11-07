# Stopped identity preparation: not runtime activation

Migration0067 and `identity_upgrade` provide an explicit stopped upgrade path ending
at **PREPARED**, never active sessions or network readiness. Ordinary legacy operation
remains the default. No CLI, server startup selection, producer fallback or automatic
reauthentication cutover is enabled. Do not invoke preparation on a live deployment.
It deliberately signs out every existing session and leaves the stores unavailable
to legacy application opens until the complete activation bundle is implemented.

## Actual owner and layout installation

`StoppedIdentityUpgrade::open_for_install` acquires the EXISTING raw redb file before
connecting SQLite. It never calls KvStore::open, SessionStore::ensure_table,
Database::open, key creation or resealing. Default pinned redb4.4 opens an exclusive
writer on supported Windows/Linux/macOS file-locking platforms; other platforms are
refused. Existing native handles and derived SessionStore handles prevent this owner.
This is not a caller boolean, process-list guess, file-marker receipt or second mutex.
It assumes supported filesystems enforce those native locks, not hostile file owners
or coordinated file replacement. SQLite-only privileged readers are outside its proof.

Before installation, actual SQLite tables must have the supported STRICT identity
shape and required primary/foreign keys. Its applied migration rows must be an exact,
successful prefix of the compiled immutable migration/checksum set, at least pinned
main's version59. Unknown future versions, holes, dirty flags and wrong/oversized
checksums refuse. Migration metadata is a trusted software-history contract, not
cryptographic authentication against an owner able to forge schema/history/files.

`install_layout` retains that owner in one operation task. Under BEGIN IMMEDIATE it
revalidates history and legacy mode, completes bounded KV inspection, then runs the
embedded migrations inside the outer SQLite transaction using nested savepoints.
Any no-transaction migration is unsupported. The whole suffix/history commits once;
failed/overbudget KV inspection reaches no migration. Installation keeps credentials
and sessions unchanged and creates no sealing key. It returns the same stopped owner,
not readiness. Already-installed67 stores can use the narrower `open` constructor.

## Modes, phases and restart

| Persisted SQL | Persisted KV | Explicit result |
| --- | --- | --- |
| legacy | no protocol checkpoint | Complete scans, choose/verify pair, write SQL preparing. |
| preparing | no checkpoint | Verify the fixed SQL history/floor/count and pair, resume invalidation. |
| preparing | exact prepared checkpoint | Verify empty session authority, finish the same SQL rebase. |
| prepared | exact prepared checkpoint | Verify exact rebased accounts/counter/pair; allocate nothing again. |
| prepared | missing/older/different checkpoint | Refuse; never recreate or adopt the rolled-back KV state. |
| unknown, malformed or other mismatch | any | Refuse; retain both stores for explicit reconciliation. |

The first immediate SQL transaction freezes writers while collecting authoritative
SQL history and the complete KV scan. Preparing stores one fixed random generation,
accepted pair, observed epoch floor, original account count and old-intent cutoff.
Existing matching pair identities remain immutable; one-sided legacy pairs refuse.

One `Durability::Immediate` redb transaction invalidates current/pre-index sessions,
account indexes and epoch sidecars together and publishes that exact checkpoint/pair.
The next SQL write transaction raises the global allocator above all accepted history,
rebases existing logins to distinct newly allocated epochs in account-ID order, verifies
the resulting rows/counter and commits PREPARED. Batches hold at most256 account IDs;
password hashes, MFA, provider bindings, PATs, keys, email proofs, locale and profile
fields are untouched. A failed SQL rebase rolls back every allocation/account change
and leaves preparing for explicit resume. Cross-store operations are not atomic.

The retired-intent cutoff is fixed upgrade-invalidation DATA. Old rows are retained;
no generic completed_at/outcome/cleanup_pair is forged or restamped. Future accepted
recovery must explicitly honor this generation-scoped cutoff. Existing generic recovery
queries are not silently broadened to clear history. A parsed checkpoint cannot mint
a PreparedIdentityPair: successful acceptance retains the actual SQL connection and
exclusive KV backend, verifies both stores and exact contiguous account/counter state,
and exposes only aggregate summary data. It is not an authentication receipt.

## Complete scans and resource limits

Explicit nonzero limits bound per-record decode bytes and accepted entry counts.
SQL account/intent counts are checked before full history aggregation; invalid counters,
sequence/history shapes and exhausted epoch space refuse. KV scans all known session,
index, legacy and binding tables, including bindings/records for deleted SQL accounts
and orphaned bindings absent from the index. Every binding contributes to the floor.
Well-formed orphan authority is explicitly invalidated, never adopted. Malformed values,
wrong schemas, unknown authority versions and incomplete scans refuse before preparing.
No prefix/sample/truncated scan is accepted as complete, and no PHC is parsed or hashed.

Caller working memory is bounded by limits, fixed metadata and small SQL batches.
Total scan/rebase/transaction time is proportional to accepted store size; redb repair,
transaction/page-cache behavior and filesystem IO are not fixed latency or RAM proofs.
Limits are operation inputs, not new production password/session acceptance policies.

## Compatibility, ownership and operational contract

New ordinary SQLite connections register the layout-v2 collation but a DENY fenced
writer marker. Only the private stopped connection receives the separate write marker.
Migration0067 protects the same13 identity/account authority tables as0061 plus the
allocator and pair table. Released-v1/cached statements lack the new layout protocol;
new unfenced pools also refuse protected writes once preparing/prepared. Persisted
mode cannot be reset/deleted through the ordinary connection. Reads/unrelated table
writes remain outside this compatibility barrier; it is not authorization against
arbitrary SQL/file owners. Normal SQL opens probe mode BEFORE migrations and resealing;
normal KV opens refuse protocol state BEFORE legacy cleanup/table initialization.

These guards add real protected-write work: layout comparison and bounded singleton
mode lookups, plus fenced comparison outside legacy mode. SQL work counters do not
make internal trigger work free. Normal startup adds bounded metadata probes; ordinary
session/page reads gain no new query or full scan. No latency improvement is claimed.

Install, prepare and acceptance run as one private owned task each. Cancelling the
waiter detaches it; actual stores and blocking KV worker remain retained until the
operation finishes. An ordered SQLx worker ping follows queued rollback before returning
an ordinary result. Failures retain the real owner for explicit retry; abnormal join
loss retains the KV guard but cannot reconstruct a lost connection. No abort handle is
exposed. Runtime destruction/panic/IO failure never yields acceptance or a durability
claim. A future host must retain/drain admitted operations before runtime exit; this
source API alone is not an installed CLI shutdown protocol.

Stop every application, background purge and cooperating writer before installing67;
retain a consistent stopped SQLite/KV/sealing-key backup. Installing67 already changes
writer compatibility even in legacy mode; it does not itself log users out. Explicit
preparation ends legacy sessions and requires the announced one-time sign-in. Rollback
requires the complete accepted pre-upgrade snapshot, not a marker/counter reset or an
SQLite rollback retaining later KV. Erasing both histories cannot prove a lost numeric
high-water mark; no arbitrary restore, cloned-file origin or global past-history claim
is invented. Same consistent snapshots still require explicit accepted generation and
session invalidation before future runtime selection.

## Prepared acceptance and remaining closed bundle

Temporary-store fixtures prepare pinned-main layout installation, unknown/dirty/checksum
history refusal, native/derived KV-handle exclusion, no migration after invalid/overbudget
KV, cancelled installation ownership, all phase resumes, SQL rebase rollback, orphan and
deleted-account floors, unchanged credentials, no forged old ACK, mismatched/rolled-back
KV refusal, scan corruption/limits/exhaustion, pre-key/pre-cleanup refusal and opaque
checkpoint behavior. Rust/SQLx/redb tests are UNRUN during source preparation. A separate
Python in-memory SQLite smoke passed schema/projection/trusted-schema/default-denial
checks only; it is not proof of SQLx migration nesting or native redb ownership.

Root gates: focused storage `identity_upgrade::tests`, all storage tests, actual supported
platform file-lock/cancellation fixtures, strict Clippy, registered AST components,
physical/dependency checks and complete legacy services/web suites. No new dependencies,
workspace registration or lock edits are required. Migration0066 remains the wiki slot.

Still required before activation: all globally allocated creation/credential producers;
all password/provider/signup/MFA/passkey original-proof origins and pending owners;
one accepted fenced issuer/resolver and every renew/logout/revoke/list/purge; atomic
credential intents and owned finalizers; accepted cutoff-aware recovery; account deletion
and all protected administrative/preference/CLI writes; generation/store binding of
issuance evidence; startup readiness/backup/rollback/reauth UX and runtime drain gates.
Ordinary pools must not simply receive the private marker while legacy APIs remain
reachable. Existing original receipts are trusted composition facts, not cross-store
or restored-generation authority. See the [closed bundle](identity-activation-bundle.md).
