# Dormant epoch-bound session sidecar

This is an inactive alternate redb persistence seam after the credential-intent and
original-epoch SQL snapshot foundations. No production session application, default
SessionStore API, SQLite credential writer, startup reconciler or authentication route
uses it. The default session tables and postcard SessionRecord bytes are unchanged;
opening KvStore or obtaining the alternate handle does not create a sidecar table.
This seam alone provides neither password-revocation durability nor crash recovery.

## Binding and transaction contracts

`SessionStore.epoch_sidecar()` returns an EpochSessionStore sharing that redb database.
An explicitly used insert writes the existing record, its `(account,digest)` index and
`session_epochs_v1` in ONE redb write transaction. Each fixed-width sidecar is 32 bytes:
account i64 big-endian, checked nonnegative credential epoch i64 big-endian, and a
16-byte random incarnation. No PHC, token plaintext or SQL lookup is involved.
The provided account must match the record; the epoch is the original proof owner's
fixed fact, never a fresh SQL epoch read by this store.

The incarnation distinguishes delete/reinsert of identical record bytes at the same
account/epoch, which an account+epoch comparison alone cannot distinguish. Random
failure is returned before opening the write transaction. Incarnation uniqueness is
probabilistic, not a mathematical guarantee; this is a CSPRNG-generated 128-bit value.
It is not a cookie, authentication proof, latest-generation authority or public Serde
receipt. EpochSnapshot's fields are private; callers can inspect the record/epoch and
explicit legacy status, but cannot construct a mutation snapshot. Debug redacts its
binding, digest, client metadata and incarnation; the store's Debug does not print a
private database path.

`put_if_absent` refuses existing records, sidecars (including orphans) and the exact
matching account-index key. It never overwrites or reindexes an existing digest and
has no upsert fallback. Every generated digest must be fresh; collision causes the
issuing application to retry token generation under a separately bounded policy.
No token retry policy is introduced here.

`get_live` reads record+sidecar from the SAME redb read transaction. Account binding
and epoch decoding are checked; wrong schema/malformed values fail closed rather
than falling back. Expired/missing records return none without lazy write cleanup.
Missing sidecar/table is explicitly LegacyInitial at fixed epoch0, not a lookup of
current SQL state. An actual sidecar with epoch0 is distinguishable from missing.
LegacyInitial snapshots cannot renew/delete/adopt through this seam. Future legacy
adoption requires its own reviewed activation interlock and snapshot comparisons.

`renew` and `delete` compare the captured account+epoch+incarnation and immutable
record identity (account/created_at/client address/user agent) under their write
transaction. Renewal rechecks liveness and only moves expiry/last-seen forward; it
does not upsert a revoked record, refresh the epoch or recreate a missing binding.
Concurrent renewals of the same incarnation can both advance monotonically. Stale
renewal/logout after delete/reinsert cannot alter the new incarnation, even with
identical old record bytes. Delete removes record/index/sidecar in one transaction.
An unexpected missing matching index entry makes mutation fail rather than silently
repair derived state.

## Index cost and explicit integrity gap

The hot read does not scan the account index or count sessions. Record+bound sidecar
are the per-digest authority; mutations perform direct `(account,digest)` B-tree lookups.
These are indexed membership operations, not an O(session-count) scan. Insert's
collision check covers that matching account key and the authoritative record/sidecar;
it does not search every other account for a corrupt orphan index key with the same
digest. A corrupt stale key under another account may remain derived garbage. This
seam has no listing API and does not turn that stale key into authorization. Activation
must validate/reconcile derived indexes in bounded maintenance and ensure listing and
cleanup recheck actual record ownership. Do not claim the current legacy index API
already supplies those stronger corrupted-state guarantees.

## Mandatory activation interlocks still missing

Existing legacy put/renew/delete/account-wide deletion/expiry purge do NOT maintain
this sidecar. They remain unchanged to avoid silently activating a new protocol or
adding hot-path work. Mixing those writers with epoch-bound operations is unsafe:
legacy overwrite can leave the old binding attached to changed bytes, and identical
byte overwrite/ABA cannot be detected by the partial immutable-identity check. Legacy
removal can leave an orphan sidecar. A successful isolated test does not fix that gap.

Before production use, route EVERY creation, renewal, replacement, logout, account
revocation, max-age and expired cleanup through the fenced lifecycle, with explicit
mode/version admission preventing old executables or unfenced writers reopening the
store. Shared clones/reopen must respect that persistent interlock. No in-memory flag
or editable successful-test record is sufficient authority. Keep old API signatures
only through reviewed compatible adapters that obey the same transactional lifecycle;
do not implement a second unguarded writer path. Legacy migration/adoption must not
relabel old sessions to the current SQL epoch or resurrect deleted records.

The opaque original-proof receipt must still flow from pre-verification PHC+epoch
through MFA/providers/signup and all remote cookie issuers. One-query SQL session
resolution must compare the captured sidecar epoch and CURRENT-target pending barrier.
Self-service keep-current requires a separate fixed-intent/fixed-epoch live-record
promotion operation and conservative overlap semantics; this seam has no promotion
or all-clear operation. Reset/admin supersession, failed partial promotion and startup
recovery still need the SQL epoch advance/intent protocol and tracked ownership before
session admission. Delayed physical cleanup must not delete newer epochs/incarnations.
All SQLite/redb commit boundaries, failures, cancellation and restart need full fault
acceptance before simultaneous activation. There is no cross-store atomic transaction.

These methods await spawn_blocking workers, but dropping the caller does not stop
those workers. The future activated lifecycle must retain counted ownership until
the actual transaction worker settles, including cancelled requests and shutdown.
The value snapshot also carries no database-instance identity: a copied database
with identical records and bindings can match it. It is a storage comparison value,
not a store-instance authentication receipt. The eventual proof owner must keep its
store/issuer provenance separate and cannot infer it from EpochSnapshot alone.

## Prepared acceptance

Fourteen actual temporary-redb tests are prepared, not run while the shared compiler
lane is occupied. They cover exact postcard/three-table binding, maximum epoch and
close/reopen, explicit/drop rollback and read isolation, injected random failure,
account/collision refusal, zero-vs-missing legacy behavior, expiry, redacted Debug,
concurrent insertion, monotonic overlapping renewal/delete, identical-byte same-epoch
ABA, stale promotion/immutable identity, malformed account/epoch/wrong table schema,
orphan collisions and direct index-membership mutation checks. Rollback tests are not
an injected redb commit-I/O-failure or power-loss durability experiment.

```text
node scripts/workspace.mjs test -p rbit-storage --locked --lib kv::sessions::epoch::tests
node scripts/workspace.mjs test -p rbit-storage --locked --lib kv::sessions::tests
node scripts/workspace.mjs clippy -p rbit-storage --all-targets --locked -- -D warnings
```

The full SQL/redb/receipt/reconciliation activation tests remain required. No source
work reduction, measured hot-path latency, durable enforcement or production adoption
is claimed by this dormant storage atom.
