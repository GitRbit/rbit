# Inactive durable credential-intent storage foundation

Migration 0060 and `sqlite::credential_revocations` are a storage foundation only.
Existing ProfileStore and RecoveryStore password transactions, legacy APIs, authentication
lookup, redb sessions and SessionUser query are unchanged. Opening a database applies
the additive schema; no production caller stages or completes these intents. This
commit does not fix credential/session crash recovery or change password acceptance.
The existing migrate/optimize methods move unchanged into sqlite/schema.rs to keep
schema responsibilities separate and reduce the oversized Database module.

## Transaction contract

`user_logins.authentication_epoch` is a checked nonnegative signed 64-bit integer,
initially zero for existing and newly inserted users. `stage` requires the caller's
explicit SQLx transaction, increments its account epoch with a maximum-integer guard,
and inserts a pending intent in a nested savepoint. Both become committed only with
the caller's outer transaction. In future activation, the password replacement and
reset-proof writes must enlist in that EXACT transaction. A failure requires aborting
the entire credential transaction; the savepoint additionally prevents insertion
failure leaving a lone epoch increment. Missing accounts/exhausted epochs fail, rather
than wrapping or returning a successful no-op. Returned facts are provisional until
outer commit; they are not authentication receipts.

The STRICT intent table stores only stable ID, account, fixed target epoch, creation
time and nullable completion outcome/time. A pending row has neither completion field;
a completed row has both and one fixed outcome. Foreign-key cascade preserves account
deletion. The account/epoch pair is unique. A partial index supports bounded pending
sweeps; each call returns at most the caller's nonzero-u8 limit (255 rows). A retry
sweep restarts at ID zero, rather than permanently overlooking older unresolved rows.

`complete` compares intent ID/account/target and requires the account's current epoch
to equal that FIXED target. It never selects a new target, completes sibling intents,
or clears an account's entire barrier. A matching already-recorded outcome returns
AlreadyApplied without repeating effects; missing/foreign/stale/conflicting decisions
return NotCurrent. AlreadyApplied describes a historical row, never authorization for
current session promotion or another cleanup. A pending older intent remains pending
when a later one completes. Callers must prove the actual redb decision separately.

AUTOINCREMENT deliberately adds sequence-maintenance cost to prevent reuse of committed
intent IDs after future completed-row cleanup. Rolled-back IDs can be reused and must
never escape as committed receipts. Do not manually assign IDs, reset sqlite_sequence,
or rebuild the table without preserving that sequence. Restoring an older database
has different rollback semantics and cannot promise globally unique IDs across history.
Completed rows currently grow without retention; this foundation ships no cleanup API.
See SQLite's [AUTOINCREMENT](https://sqlite.org/autoinc.html) and
[STRICT tables](https://sqlite.org/stricttables.html) contracts.

## What remains before activation

A future session read must compare the ORIGINAL session epoch with SQL's current epoch
and reject while ANY unresolved credential intent exists, in the existing one-query
SessionUser projection. A versioned redb epoch sidecar must be updated with each session
record/index in the same redb write transaction, preserving legacy postcard bytes.
Missing legacy sidecars mean fixed epoch zero, never the latest SQL epoch.

Password verification must capture an epoch with the PHC BEFORE verification and carry
that same opaque proof through local MFA and cookie issuance. Reading the latest epoch
after verifying old password bytes would still admit the stale-password -> reset ->
late-session race. Signup, external-provider proof, all remote session issuers and the
explicit trusted-User embedding compatibility API need their specified provenance.

Self-service keep-current requires an existing live authenticated redb record, promotion
to the exact committed epoch, epoch-filtered deletion and fixed-intent acknowledgement.
Crash after partial promotion must end all by advancing SQL again before resolving
pending intents; simply clearing pending would revive the partially promoted session.
Forced reset/admin supersession, overlapping guards, late cleanup and conservative
restart reconciliation still require dedicated transactional operations and fault tests.
`complete` alone does not implement those operations. Startup must finish recovery
before admitting sessions, with failed recovery preventing activation. Existing tracked
finalization continues to prove in-process lifetime only.

Activate writers, reader fence, original-proof propagation and startup recovery together,
with old executable rollback explicitly prohibited once active: old readers ignore the
fence. Preserve the current self-service/overlap/end-all semantics through fault tests,
including every commit/redb/acknowledgement boundary. SQL/redb are separate transaction
authorities; WAL NORMAL also does not guarantee preservation of the last acknowledged
commit after power loss. No cross-store atomicity, power-loss or durability claim follows
from this schema alone. Common hot paths pay no additional queries in this inactive slice.

## Upgrade and rollback

Migration 0060 runs even while runtime fencing is inactive. An older executable's
migrator refuses a database containing this unknown version; keeping an older binary
alone is not a rollback plan. Before testing an upgrade, stop and drain the owned
instance, then preserve a consistent complete data snapshot, configuration and original
sealing key. Never copy live SQLite/redb files. To return to an older build, stop the
new instance and restore that pre-upgrade snapshot as a unit. Preserve later data
separately; restoring a snapshot does not retain post-upgrade writes. Do not delete
migration-history rows or edit the released migration to force a downgrade.

The upgrade fixture checks the older migrator's version refusal and its successful
opening of a closed pre-0060 database copy with unchanged account/password rows.
Complete platform backup/restore and activated-protocol rollback acceptance remain
separate gates; this fixture is not a rehearsal of every persisted store.
## Validation prepared

The integrated storage test binary adds 11 focused cases covering migration from 0059,
row preservation/default-zero, close/reopen persistence, foreign-key/integrity checks,
STRICT/uniqueness failures, password+proof+intent commit/rollback, failed insert savepoint,
failed later statement/outer commit, maximum-integer exhaustion, fixed-epoch/foreign and
conflicting completion, bounded pending pagination, completion rollback, committed ID
nonreuse after simulated completed-row cleanup, account deletion, and unchanged legacy
password-writing behavior. These are temporary fixture databases, never live credentials.

Rust tests and strict Clippy remain required for this integrated storage slice.
Run these through the shared compiler lane:

```text
node scripts/workspace.mjs test -p rbit-storage --test integration credential_revocations --locked
node scripts/workspace.mjs test -p rbit-storage --test integration upgrade --locked
node scripts/workspace.mjs clippy -p rbit-storage --all-targets --locked -- -D warnings
```

The full protocol still needs real redb/SQL fault, cancellation, restart, concurrent
session-issuance and one-query regression tests before production adoption.
