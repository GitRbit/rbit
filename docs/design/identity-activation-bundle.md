# Closed durable session-fencing activation bundle

This is an implementation order for one coherent security upgrade, grounded in public
`07b3a7e9` and the queued changes below. Nothing in this document selects a new route,
changes a session writer, or claims that the current dormant pieces enforce recovery.
The minimum supported existing-store policy is explicit **one-time reauthentication**:
end legacy sessions during a stopped upgrade, preserve accounts and every authentication
method, and accept new sessions only after both stores agree on the active protocol.
There is no safe requirement to adopt ambiguous legacy session records.

## What is prepared, and what it actually closes

| Source/change | Prepared ownership; remaining condition |
| --- | --- |
| `accounts/password_authentication.rs`, identity `accounts/authentication/proven.rs` (public original-proof code) | Same original PHC/epoch snapshot before verification; sealed non-Serde receipt after success and suspension precedence. Trusted repository/composition evidence, not cryptographic or store-instance authority. |
| `two_factor/password_sign_in.rs` (this change) | One shared legacy/original throttle/verification/audit worker; original password proof reaches no-MFA completion or the existing receipted pending owner. No route selection. |
| `two_factor/pending.rs`, `two_factor/sign_in.rs` (public original pending) | No legacy downgrade or original epoch refresh; recheck the fixed epoch before local MFA. |
| `edbe442` (replayed as `15ea48a2`) | TOTP/recovery/passkey one-use writes share the exact SQL transaction with original-epoch/current-pending/suspension predicates. `d64ddc6` makes the new direct fixtures compatible with the writer marker. |
| `a529e6b` (public sidecar) | Record/account/epoch/incarnation binding, insert-only put and captured-binding renew/delete CAS in one redb transaction. Dormant; not a complete session repository or instance-ownership receipt. |
| `5191e90`, `b326ff2` (public intent/read foundation) | Same-transaction local epoch/intent primitives; one indexed fixed-epoch account projection. Local increment is insufficient for account-ID reuse. |
| `dc2b989` (queued allocator) | Checked global allocation, SQL-history seed and explicit floor raise in caller transactions. Its migration must become0063 after writer0061 and locale0062. It is not yet used by all creation/mutation paths and does not scan surviving redb history. |
| `73efc046` + `03f3281e` (queued creation producer/test correction) | Creation epoch returned from the SAME insert transaction, receipt minted after commit. Must switch insertion to global allocation before use. |
| `4057501f` (queued provider producer) | Capture first accepted linked-account/verified-email origin, retain through provider/MFA/registration flow and final fixed-epoch projection. Must retain nonce/PKCE/email/link/trusted-provider policy and use globally allocated creation epochs. |
| `profiles/fenced_password.rs`, `recovery/password_reset.rs` (prepared transaction writers) | Global allocation, exact password/reset proof mutation and pending intent share one commit. No service/finalizer selects them yet; legacy reset eligibility remains shared. |
| `827204f` (replayed as `f3fa748e`, queued writer0061) | Missing-v1 official legacy writers refuse protected DML, even cached before migration. **Released v1 binaries still register v1 and retain unfenced writers. Fenced activation needs a new persisted required writer protocol.** |

These are source preparations; their individual tests and the complete bundle still
need compiler/runtime acceptance on the shared lane. Successful isolated primitives
are not evidence that production routes are fenced.

## Ordered changes that must ship together

1. **Persistent mode/version and stopped upgrade owner.** Add append-only SQL protocol
   state and redb protocol state with the same accepted generation identifier. Unknown,
   malformed, missing-on-one-side, or incompatible state refuses admission; no automatic
   fallback to the current legacy reader. New compatible binaries must register the
   old marker only to perform a supported migration, then enforce a newly required
   persisted writer version on protected DML. Old v1 connections must fail after that
   trigger transition, including cached statements. Do not mistake version capability
   for authorization against arbitrary SQL/file access.

   The upgrade owner must run before `Services::new`, listeners, background cleanup,
   or offline account creation. Current entry points are server `config.rs`,
   `serve.rs::{open_database,migrate,create_user}` and `main.rs::run`. `KvStore::open`
   currently calls `SessionStore::ensure_table`, which creates current tables and
   drops legacy session state: provide an inspected upgrade-opening path before
   those side effects. A marker alone cannot stop a process holding an older redb
   handle; exclusive ownership and stopping that process are mandatory.

2. **Bounded history inspection and explicit invalidation.** Before any credential
   producer allocates, inspect all SQL epoch/counter/intent/sequence state plus every
   redb binding, including orphaned bindings absent from the account index. Bound
   memory by iteration/batches; never truncate a scan and call it complete. Refuse
   unknown table formats, malformed values and exhausted epoch space. Raise the
   global allocator floor above all surviving authority and rebase existing accounts
   to distinct newly allocated epochs. Deleted/recreated account IDs must never regain
   their old credential epoch. Supported SQL-history assumptions in `dc2b989` do not
   prove arbitrary manual edits or partial rollback safe.

   Persist SQL `preparing` before a redb transaction invalidates all legacy/current
   session records, account indexes and epoch sidecars and publishes matching prepared
   state. Rebase/activate SQL under the upgrade owner, then accept only the matching
   prepared/active pair. Define restart for every partial publication; no listener
   starts merely because one store says active. This is a recoverable protocol,
   not a cross-store atomic transaction. A complete stopped backup remains required.

3. **One concrete credential-mutation transaction per operation.** Replace the active
   legacy store calls at `profiles/password_changes.rs::change_password` and
   `recovery/password_changes.rs::{reset_password,admin_set_password}` with explicit
   global-epoch+pending-intent operations. In `sqlite/profiles.rs` keep the old-PHC CAS
   for self-service. In `sqlite/recovery.rs::reset_password` preserve reset-link
   eligibility, one-use consume, initial-password policy and password-change-required
   updates. The existing password write/consume, global allocation, login epoch update
   and intent insert must commit or roll back together. Do not append an intent after
   committing the existing password update. Return the fixed committed intent to the
   existing single owned `credentials::finish_update` worker; do not reread the latest
   epoch or create a second detached task.

   Active mode must also refuse legacy public mutation entry points or route them
   through the complete transaction. Merely giving a new binary the v2 collation
   does not stop its own unfenced legacy APIs. Protect mode decisions at the mutation
   transaction/lifecycle boundary, not a stale optional caller flag. Preserve eager
   zeroizing inputs and validation/throttle/auth/admission/CAS/consume precedence.

4. **Complete epoch-aware session repository and finalizer.** Extend the dormant
   `kv/sessions/epoch.rs` into the sole active session mutation path. The entire set
   matters: insertion, single logout, account-wide revocation, revoke-others,
   list/revoke metadata, renewal/last-seen, expired cleanup and account deletion.
   Legacy `kv/sessions.rs::{put,renew,delete,delete_for_account,purge_expired}` cannot
   keep mutating records/indexes without maintaining sidecars. Reject missing sidecars
   in active mode; INITIAL is not authority to adopt a new epoch.

   Each late mutation matches the captured incarnation/account/epoch and record
   identity. Never recreate a removed record, restore an old binding or stamp it with
   a fresh SQL epoch. Expired cleanup must recheck under its write transaction.
   Surviving orphan/unknown records are recovery work, not silently accepted evidence.
   Retain ownership of the actual blocking redb worker after caller cancellation;
   dropping its awaiter alone does not prove the transaction finished.

   Reset/admin intents end all sessions. Self-service may promote **only** its captured
   still-live current incarnation to the fixed committed target epoch, provided the
   latest authoritative SQL fence still accepts that target; a newer reset wins.
   Never promote from an arbitrary digest or keep a replacement record. Concurrent
   profile changes retain the existing old-PHC CAS and overlap/no-resurrection policy.
   Admission may remain generically unavailable while the current intent is pending.
   A later SQL change racing a fixed-target promotion makes that record stale; it must
   never cause promotion to the newer target.

5. **Bounded recovery and supersession.** Consume the actual committed intents from
   `sqlite/credential_revocations.rs` and reconcile redb idempotently before exact
   current-target acknowledgment. Existing `complete` refuses older targets on purpose;
   do not loosen it or clear all pending rows. Explicitly supersede historical intents
   only after a newer authoritative transition or the accepted upgrade invalidation
   proves their remaining session authority retired. Use an indexed current-target
   account projection; bounded sweeps must not starve behind permanent old IDs.
   No success acknowledgment after unproved redb failure, panic or cancellation.

6. **One active issuer and one active resolver.** Add the epoch-aware application port
   behind `accounts/sessions.rs` and `identity/sessions/application.rs`, using sidecar
   read snapshots and the existing one-statement
   `AccountStore::find_session_user_at_epoch` projection. Resolve current active user
   flags/notification/locale data from that query; attached mutable `User` flags are
   not authority. Keep the original receipt fixed through final issuance. An epoch
   changed after old-password verification cannot mint a session at the newer epoch.
   A reset after the last SQL check may leave an inserted *old-epoch* record, but the
   resolver must reject it; it must not be rehabilitated by a fresh epoch lookup.

   `start_session(&User)`/`start_session_for(&User,...)` remain compatibility APIs only
   in legacy mode (or an explicitly separate trusted embedding composition). Active
   remote routes must not reach them. Resume still performs one account SQL query;
   make any extra final issuance query explicit and measure its issuance-only cost.

7. **Switch all production web producers in that same release.** The exact current
   User-only session path is `routes/auth.rs::start_session` calling
   `accounts.start_session_for`, followed by existing cookies/theme/forced-password
   redirect behavior. Replace that shared issuer and all callers together:
   `routes/auth/password_commands.rs::{login,signup}`;
   `routes/security/password_commands.rs::complete_sign_in`;
   `routes/external_auth.rs` callback completion and external registration.
   Password login uses `sign_in_original`; signup uses the queued creation receipt;
   MFA uses `complete_original_sign_in`; provider completion/registration uses the
   queued original provider workflow. Preserve trusted-provider MFA bypass and all
   state/nonce/PKCE/linking/email checks. No OAuth nonce is a password proof.

   Audit all passkey sign-in challenge/finish and settings step-up owners: current
   passkeys are a second factor, so their local sign-in path must retain the same
   original pending receipt and the `edbe` write predicate. Settings proof changes
   must retain their separate authorization and one-use semantics. Password-confirmed
   account rename/email/MFA mutations are not session issuers; do not accidentally
   treat their fresh User reload as an original-proof receipt. Suspension/reactivation
   must preserve current policy and authoritative flags, without epoch restamping.
   PAT/Git/SSH authentication, existing PAT scopes, keys and recovery credentials are
   retained; they do not acquire cookie-session authority through a User-only escape.

8. **User-visible and rollback contract.** Announce a one-time sign-in after the stopped
   security upgrade; expired/legacy cookies route through the ordinary sign-in UI with
   a localized upgrade explanation. Do not delete accounts, hashes, MFA enrollment,
   remaining recovery codes, passkeys, provider links, PATs, SSH keys, preferences or
   repository features. An authenticated in-flight request may have authorized against
   a pre-commit SQL snapshot; do not claim retroactive cancellation of all such work.

   Downgrade restores an accepted consistent pre-upgrade SQL+redb+key/config snapshot;
   never edit migration history, remove guards or mix SQL rollback with newer redb.
   Such a restore can restore old session cookies: provide an explicit tested logout
   step on the restored copy when that is unacceptable. No arbitrary partial restore
   is declared safe, and no durability guarantee extends beyond the acknowledged
   storage/backup guarantees.

## Required end-to-end gates before production selection

Run actual-store/process tests for every upgrade publication cut and every credential
commit/cleanup/promotion/ack cut; restart/retry must stay fail-closed and preserve
credentials. Include already-open cached v1 writers, current-binary legacy API refusal,
new connections, deletion/recreation and retained orphan sidecars, rollback/counter
exhaustion, stale pre-verify password/provider receipts, stale MFA after its SELECT,
logout/purge/renew ABA, overlapping self-service/reset, caller cancellation while the
blocking worker continues, and a newer reset racing current-session promotion.

Exercise every production producer through HTTP (no-MFA, TOTP, recovery and passkey
MFA, signup, linked provider, verified-email match, provider registration and trusted
provider bypass), plus logout/settings/revocation and restart. Preserve existing
error/503 privacy headers, audits, request budgets and no-JS flow. Verify notification
and locale projections remain a single account SQL acquisition/statement. Full tests,
strict Clippy, model/token-only dependency guards and measured representative request
costs are gates; no hidden per-request mode/counter scans or dummy crypto are allowed.

The concrete store operations in step3 are now prepared in
[transaction-owned credential changes](identity-credential-mutation-transactions.md),
including committed intent return and failure rollback. Their service selection is
still blocked on the actual finalizer/recovery protocol. The **next directly
completable implementation** is step4's complete epoch-aware repository lifecycle:
account-wide revoke/keep-current promotion, list/revoke/cleanup maintaining captured
bindings, then step5 recovery and step6 issuer/resolver through that repository. Only
then bind the three credential services to their new store operations inside the
existing owned finalization worker and switch step7 together. There is no safe
route-only activation. Until those gates close this bundle remains unactivated.
