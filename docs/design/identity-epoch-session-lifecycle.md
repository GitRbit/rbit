# Epoch-aware session lifecycle adapter

Status: explicit alternate application/service APIs only. Production routes,
middleware session resolution, default legacy session methods, startup mode and
background cleanup remain unchanged. This is not a completed fencing upgrade.

The used composition is `AccountService`'s original-session methods →
`EpochSessionApplication` → pure model-owned session/account ports → actual redb
`EpochSessionStore` and one-statement `AccountStore::find_session_user_at_epoch`.
It uses the queued locale-aware projection without a second preference lookup.
Only the sealed identity-minted `OriginalProofReceipt` implements issuance proof;
there is no public User/current-epoch constructor or Serde receipt. INITIAL and
unbound snapshots are refused, not adopted. Returned user flags come from the fixed
original-epoch SQL projection, not mutable fields attached to first-factor proof.

Issuance makes its final fixed-epoch SQL check once and returns the inserted captured
binding directly from the same redb insert transaction, without a later adoption read.
A reset after that SQL check can race an old-epoch insertion: the resulting cookie
cannot resolve against the new SQL epoch. No atomic SQL/redb issuance claim is made.
The final lookup costs one SQL statement/acquisition at issuance. Resume has the same
one account SQL statement/acquisition boundary as the legacy resolver, including
password-change-required, notification and account-locale fields.

Common live resume is one captured record+sidecar read, then that SQL projection. It
adds no account-index scan, mutation owner task, metadata clone, renewal write or
per-request mode/counter read. Max-age rejection keeps its pre-SQL order; expired
records wait for cleanup. Renewal preserves the original idle/max-age/last-seen
thresholds and monotonic timestamps, with an exact captured-incarnation CAS.
Listing moves owned metadata into existing ActiveSession views without per-row record
clones. Single revoke and revoke-others use the captured epoch and incarnation; an old
request cannot remove a newer epoch's sessions. Revoke-others validates the live current
incarnation and deletes other same-epoch records in one write transaction. Its index
walk uses bounded key batches without rescanning earlier keys; transaction duration
still scales with the account's session count. No new session-count cap is introduced.

Expiry maintenance reuses the existing 256-key batch size. Each write batch rechecks
binding, immutable identity and current expiry, so a renewed/replaced record survives
a stale scan. Deletion removes record/index/sidecar together. Missing/corrupt bindings
or inconsistent account-index ownership fail; no legacy repair/adoption occurs.
The hot read deliberately does not prove global index completeness. Startup still
needs a complete record/index/sidecar/orphan audit and an actual legacy-writer barrier.

Explicit mutation adapters use the existing per-instance session scope. Insertion,
rare renewal, logout and purge reserve before their mutator's first await and retain
the actual blocking-worker join. Whole single/other revocation and its audit are in
the same admitted worker. Reads/listing do not reserve. Consuming snapshot variants
avoid a second metadata clone across owned-task and blocking-worker boundaries.
No task captures its controller. Caller cancellation detaches its waiter; shutdown
still drains the tracked task. A prior close refuses new mutation admission; an
already-admitted operation can finish after close. Forced runtime termination remains
abnormal/unproved, not durable storage recovery.

Prepared real-store tests cover issue/resume locale/query counts, stale original
receipts after committed reset, legacy/INITIAL refusal without an adoption query,
listing/single/bulk/logout/audit, renewal/max-age, closed and unpolled ownership,
expiry purge, account/epoch isolation, same-epoch ABA, cross-account index corruption
rollback, multiple purge batches and stale expiry replacement. A deterministic owned
caller-cancellation/close fixture then completes the actual redb insert before drain.
They have not been compiled or executed during source preparation.

Shared-lane acceptance: identity sessions-only and password-work+sessions tests and
doctests; storage epoch unit tests and integration tests; services epoch_sessions and
owned-repository/credentials tests; strict Clippy and isolated model/token feature
graph checks. Preserve precise SQL work counts; owned audit measurement is explicitly
carried by the preceding completed-value ownership change.

Remaining active-mode gates are in [the closed bundle](identity-activation-bundle.md).
The separate [durable recovery implementation](identity-durable-session-recovery.md)
adds migration0064's durable decision, fixed promotion, paired acknowledgment and
supersession. Its prepared source still requires compilation and real-store gates. All credential producers, persistent v2 interlocks,
upgrade/relogin/rollback and production issuer switching must close together.
