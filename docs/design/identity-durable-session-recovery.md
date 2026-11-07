# Durable session reconciliation: inactive implementation

Migration0064 and the explicitly named original-session services implement durable
end-all and keep-current reconciliation. Current routes, credential writers,
ordinary session readers and startup do not select this protocol. This is not
active crash-safe revocation, a completed identity port or a mixed-version mode.

## Fixed authority and transactions

`ProfileStore::set_password_hash_if_current_preserving` accepts a promotion captured
from the original live sidecar snapshot before SQL. It checks original account and
epoch, then commits the password CAS, global epoch allocation, fixed 64-byte
promotion decision, intent and reset-link revocation in one SQL transaction.
Administrator/reset intent transactions persist end-all. Existing legacy APIs stay
unchanged. Capture is not a fresh SQL epoch read and cannot adopt a replacement
session. Failure before SQL commit rolls back the complete transaction; cancellation
while COMMIT is dispatched can leave a durable pending intent without a return value.

The promotion binds digest, account, original epoch and random incarnation. Recovery
only preserves that live incarnation at the original or exact target epoch. A
missing/expired/replaced session becomes end-all, never a newly adopted keep. Actual
redb reconciliation checks record/index/sidecar ownership and removes sessions at or
below the fixed target in the same write transaction, retaining sessions above it.
The retained session's postcard record bytes do not change. Replaying after a redb
commit but before SQL acknowledgment recognizes the same already-promoted incarnation.

Epoch-exact stale resolver cleanup cannot delete a promoted session. Explicit cookie
logout may delete the same immutable incarnation after promotion; replacement nonce
and account mismatch remain refused. Active legacy mutation exclusion is essential:
these contracts do not tolerate legacy overwrites that leave an old sidecar behind.

## Explicit pair, acknowledgment and supersession

Neither migration nor Services construction initializes a pair. The explicit redb
initializer refuses existing records, indexes, bindings or legacy sessions; it creates
one random immutable 16-byte identity only in an empty store. SQL pair initialization
binds that existing identity once, refusing mismatch or malformed state. There is no
automatic purge, adoption, reset or all-clear operation. Production upgrade still
requires an accepted exclusive-writer protocol before invoking these capabilities.

The concrete recovery owner verifies the SQL/KV pair before effects; actual redb
application checks its identity again inside the write transaction. Only a backend
completion for the exact request and same runtime redb owner can reach SQL ACK. ACK
checks fixed account/epoch/decision, current SQL epoch, pending status and pair, then
stamps `cleanup_pair`. A completed known decision lacking that paired proof is invalid.
Historical legacy completions are unsupported and never restamped or treated as
cleanup witnesses. Pair identity is not proof of chronological snapshot consistency
or original file provenance; restoring SQL backwards while retaining later KV remains
unsupported. A cloned matching pair requires the accepted backup/startup contract.

Older pending rows retire only after a newer current, paired, nonlegacy completion.
Merely seeing a greater epoch or pending intent is insufficient. SQL guards preserve
immutable decisions and completed results, prevent deleting unresolved authority and
retain a required newer witness until older work is superseded. Account deletion
continues to cascade. Completed history still grows: no retention policy is activated.

## Ownership and bounded work

`OriginalCredentialFinalizer::begin` reserves the nonqueued backend-owned recovery gate
before the SQL transaction, while borrowing an already-counted mutation context. It
holds that gate through the actual redb worker and ACK. Exact/batch recovery reserves
the existing mutation scope before first await; Busy does not enqueue another owner.
One batch is at most 255 selected rows. Cursor advances past deferred older IDs;
callers must restart from zero after a sweep to revisit them. A cursor or empty page
is not a readiness proof. The gate covers a bounded page/operation, not an unlimited
sweep; no Argon2, network or audit occurs beneath it. This bounds admitted owners,
selected intent count and temporary batch memory, not total per-account work: one
redb write transaction still visits every indexed session of that account. There
is no configured session-count limit. Large accounts can therefore hold the global
gate and delay shutdown; mutation latency/large-account tests remain activation gates,
not a fixed CPU/time bound or an invitation to abort a partially committed cleanup.

The gate lives on the actual `KvStore` handle and is shared by its clones and by
independently constructed Services using that backend. A second composition cannot
classify an intent while another owns cleanup. It does not coordinate another
process or independently opened storage; persistent exclusive-writer startup gates
remain mandatory. The duplicate-composition fixture also checks completed retries
cannot remove a fresh same-target session issued after paired acknowledgment.

Caller cancellation drops only its waiter. The existing owned worker retains scope,
gate and actual blocking-task completion through shutdown. Forced runtime termination
remains unproved; persisted pending decisions support a later admitted retry. After-
the-fact finalization may return Busy after SQL already committed, retaining pending
work rather than claiming rollback. Exact ordinary resume adds no pair query.

Measured-work fixtures assert exact reconciliation SQL acquisitions/statements:
first exact keep is 4/4 (pair, find, state, ACK); completed exact retry is 3/3. These
are prepared source assertions, not executed measurements or latency claims.

## Prepared gates and remaining activation

Temporary SQLite/redb fixtures cover stopped reopen after promotion/before ACK,
ACK failure and retry, wrong pair/foreign owner, immutable metadata, malformed large
blob classification, keep-decision rollback, pending/newer witness supersession,
account deletion, same-target sessions issued after ACK, cookie logout crossing
promotion, schema upgrade refusal, nonce replacement and corrupt-index rollback.
Owned service fixtures cover nonqueued admission, foreign context, cancellation and
shutdown. These new tests have not been compiled or executed during preparation.

Root acceptance must include identity sessions-only and password-work+sessions,
storage unit/integration tests (`revocation_recovery`), service original-recovery and
epoch-session tests, isolated model/token graph checks, strict Clippy and existing
legacy route tests. Files require the completed-value ownership and epoch lifecycle
commits preceding0064, plus writer0061, locale0062 and allocator0063.

Activation remains one closed bundle: persistent v2 writer/version exclusion (v1
alone is insufficient), globally nonreused creation/credential epochs, bounded orphan
scan and accepted pair/bootstrap, all local/provider/passkey/MFA proof origins, every
issuer/reader/renew/logout/revoke/list/purge, all credential transaction/finalizer
consumers, startup pending recovery and fault/rollback gates. Existing-store upgrade
requires an explicit one-time sign-in policy, intact credentials and an accepted
consistent backup; no silent legacy token adoption is implemented. See
[the activation bundle](identity-activation-bundle.md).
