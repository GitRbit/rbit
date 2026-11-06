# Dormant global credential epochs

Status: persistence primitive only. Migration 0063 seeds an allocator; no production account creation, password mutation, session reader, provider, MFA or recovery path uses it. Migration 0060 and all current writer APIs are unchanged. This atom does not fix crash recovery or account-ID reuse by itself.

## Why a global sequence is required

`accounts.id` can be reused after deletion. A newly created account at epoch zero could otherwise match an orphaned redb session belonging to a deleted account with the same ID. Activation must allocate a globally new committed epoch for **every** account creation and credential advance, including signup, external registration, offline/admin creation and reset/profile/admin password changes. Existing local `epoch + 1` and default-zero creation cannot coexist with that mode.

`credential_epochs::allocate_in(tx)` advances a checked SQLite integer in the caller's existing write transaction. The future owner must write that value into the account/intent in the **same** transaction, commit successfully, then publish a receipt/session. The primitive returns only a provisional `CredentialEpoch` data fact. It cannot mint authentication receipts. Rollback and failed commit may reuse provisional values; no receipt may escape those transactions. Successful committed allocations are never reused by normal allocation, including after account/intent deletion and process restart.

On valid state allocation adds exactly one SQL statement to a mutating transaction, no account lookup, no separate transaction/fsync and no per-request counter read. These are source contracts, not measured latency claims. Its bounded failure query inspects at most two rows. Missing, malformed or duplicate singleton state is never recreated automatically; exhaustion never wraps or promotes to floating point. A committed allocation may equal `i64::MAX`; all subsequent allocation fails. Migration and startup floor acceptance reject an exhausted starting state.

## One-time legacy seed proof and its limits

The supported pre-global history begins at epoch zero. Each committed account-local increment also inserts one automatically numbered `credential_revocations` row in the same transaction. For any account, its increment count cannot exceed the count of committed intent insertions; the persistent AUTOINCREMENT high water cannot be smaller than that count. SQLite retains `sqlite_sequence` after deleting completed intent rows or cascading account deletion. Rolled-back insert IDs are provisional and do not contribute authority.

Migration 0063 seeds at least the maximum of current account epochs, retained intent targets and that persistent sequence. It checks nonnegative integer account epochs, positive integer intent IDs/targets with `target <= id`, at most one positive integer sequence row, and a sequence not behind retained IDs or current account epochs. Missing sequence is accepted only with no positive retained history. Named migration constraints distinguish unsupported history (`credential_epoch_history_invalid`) from exhaustion (`credential_epoch_space_exhausted`). The `target <= id` check describes the supported **pre-global** history; it is not an invariant of later global allocation and must not be reapplied to an active global database.

These checks detect contradictory evidence, not arbitrary privileged database tampering. Erasing all SQL evidence and its sequence can look exactly like a pristine database. Manually assigning IDs/epochs, editing sequence/counter state, bypassing constraints or restoring SQL independently of redb invalidates the history proof. SQL migration success therefore never authorizes activation. Preserve both stores on invalid/unknown history and stop activation with an actionable operator error; do not reset counters, silently log out users or reinterpret unknown bindings as INITIAL.

## Startup floor and activation obligations

`raise_floor_in(tx, observed)` is a startup/restore data primitive that never lowers the allocator. A supplied integer, editable JSON or one observed session is **not** proof of a complete scan. The future private startup owner must hold exclusive epoch-aware writer/version interlocks, validate accepted SQL history and schema, and perform a bounded, complete redb sidecar/orphan scan. It must reject unknown/malformed schemas or epochs and rebase above every accepted observation before enabling any issuer. Redb observations can exceed the SQL seed even when the SQL database appears pristine; the temporary-store test demonstrates this ambiguity.

Still required before activation: persistent old-writer/version barriers; all creation/password producers using the allocator; original-proof receipt propagation through every issuer; authoritative fenced session reads; atomic sidecar mutation and legacy-writer/cleanup interlocks; current-intent recovery, bounded supersession, self-service keep-current/overlap semantics and end-to-end crash/cancellation faults. No all-clear or readiness API is introduced here. Restores must use consistent accepted snapshots or explicitly reset KV authority under the existing backup policy; rolling SQL back while retaining arbitrary KV state is not safe.

## Acceptance

Temporary SQLite/redb fixtures cover legacy sequence persistence after cleanup/account deletion, unsupported manual histories, exhaustion, malformed singleton state, one-statement allocation, floor monotonicity, rollback/deferred commit failure, separate connection-pool serialization, reopen, reused account IDs with retained old bindings, and an SQL-empty/redb-orphan example. They never read user data.

Prepared commands for the shared compiler owner:

```text
cargo test -p rbit-storage --test integration credential_epochs
cargo clippy -p rbit-storage --all-targets -- -D warnings
```

Run the existing old-migrator refusal, stopped backup-copy upgrade and full workspace migration tests as well. Rust tests remain unexecuted in the authoring worktree while the shared compiler lane is reserved. No dependency or lockfile change is needed. Migration 0061 installs writer compatibility and 0062 is reserved for account locale; this append-only migration is 0063. Its allocator table also requires the registered v1 writer protocol on all DML. That capability does not activate session fencing.
