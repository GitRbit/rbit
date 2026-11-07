# Closed active composition: implementation contract

This follows the PREPARED-only owner in `identity-stopped-upgrade.md`. It does not
select a production route, make a prepared deployment usable, or claim the full
activation bundle is complete. Migration0068 is reserved for the distinct ACTIVE
publication. Migration0067 and its checkpoint encoding remain immutable.

## Source inventory and required selection

The following are actual production paths on this branch, rather than hypothetical
entry points. Tests that call compatibility APIs directly are not remote issuers.

| Boundary | Existing caller and closed-bundle change |
| --- | --- |
| Password login | `web/routes/auth/password_commands.rs::login` calls `Services::sign_in`; select the shared `sign_in_original` worker. Preserve throttle, Busy, failure accounting and audit order. |
| Signup | The same file's `signup` calls `accounts.sign_up`; creation must return a sealed receipt only after globally allocated creation epoch and user insert commit together. Queued creation/provider atoms are not yet present on this base. |
| MFA | `web/routes/security/password_commands.rs::two_factor` calls `complete_sign_in`; select `complete_original_sign_in`. Existing passkey challenge/finish is a second factor, not another independent first-factor issuer. Retain original pending receipt and atomic original-proof recovery/TOTP/passkey writes. |
| Provider callback | `web/routes/external_auth.rs` calls `complete_external_sign_in`; `services/external_auth.rs` first finds the linked identity or verified-email account, then touches/links and finishes. Capture the accepted account epoch at that first authoritative account acceptance, never after intervening reset. Preserve OAuth state/nonce/PKCE, trusted-provider MFA bypass, verified email and linking rules. |
| Provider registration | The same route calls `register_external`; `sqlite/external_identities.rs::register` inserts user and link in one transaction. Allocate creation epoch there, retaining the committed receipt through the shared issuer. |
| Cookie issuance | Every remote completion above reaches `web/routes/auth.rs::start_session`, currently `accounts.start_session_for(&User)`. Replace its User-only input with original proof and use the single fixed-epoch issuer; cookies/theme/next/forced-password redirect stay shared. |
| Request resolution | `web/middleware/auth.rs::session` calls `resume_session`; select the original resolver and retain its captured snapshot in request extensions. Its one SQL projection already includes active flags, password restriction, unread indicator and account locale. No second user or mode query. |
| Logout | `web/routes/auth.rs::logout` calls `end_session`; use captured-sidecar logout. Missing/expired cookies still clear normally. |
| Settings sessions | `web/routes/security.rs::{overview,revoke_session,revoke_other_sessions}` use legacy list/revoke APIs. Use the resolved snapshot, retaining current-session checks, audit details and exact privacy semantics. |
| Password change | `services/profiles/password_changes.rs::change_password` currently commits PHC CAS then legacy finalization. Capture the initiating binding before the SQL transaction and persist its exact keep decision with global epoch/PHC CAS/intent; reconcile inside the existing owned worker. Replace middleware `PasswordChangeGuard` ownership without adding a second detached worker. |
| Reset/admin password | `services/recovery/password_changes.rs::{reset_password,admin_set_password}` use legacy password writes. Select the prepared transaction writers and EndAll decision, preserving reset one-use, initial-password, admin authorization and hash admission order. Standalone `recovery/session_effects.rs` must not retain an unfenced fallback in active composition. |
| Account creation | `accounts/registration.rs::create_user_offline`, `admin/accounts.rs::create_user`, signup and provider registration all require globally allocated epochs in their existing insert transactions. Offline SQL-only writer opens must refuse active mode unless they obtain the actual paired owner. |
| Other account authority | `sqlite/admin.rs::{set_suspended,delete_user}`, profile rename/email and MFA settings remain authoritative SQL operations. Suspension is checked on every session projection; rename never remints a receipt. Deletion must retire sidecars safely; globally nonreused creation epochs prevent reused account IDs adopting orphan bindings. Existing settings step-up is not a session issuer. |
| PAT/Git/SSH/API | Existing credential/key/scope checks remain; they must not invoke User-only cookie issuance or acquire fresh session epochs. They do not become original password receipts. |
| Expiry | `server/serve/session_purger.rs` directly calls `kv.sessions().purge_expired`; select the tracked epoch-aware purge. All delete/index/sidecar changes share their existing redb transaction and compare captured identity. |
| Runtime ownership | `accounts/epoch_repository.rs` and `accounts/original_recovery.rs` retain actual workers through cancellation. Recovery is backend-shared, nonqueued and releases per intent. Batch count and memory are bounded; account cleanup may still visit every session in one transaction. |
| Startup | `server/config.rs::load` opens/migrates/reseals SQLite before `serve.rs::open_storage` obtains KV. Split deployment location from saved-config hydration; acquire actual existing KV first, accept paired mode, then open private protocol SQL pools and hydrate settings. No listener/background owner may precede acceptance/reconciliation. |
| CLI | `serve.rs::{migrate,create_user}` currently open only SQL. Refuse active stores on this path; provide an explicit paired stopped command for supported mutations. `Database::migrate` remains legacy-only until active migration ownership is implemented. |
| Backup/restore | `server/backup.rs::snapshot_database` uses `VACUUM INTO`, intentionally omitting KV and requiring sign-in after restore. Preserve that feature with an explicit stopped restore/reauth/new-pair generation protocol. Ordinary active startup must never create missing KV, silently re-pair or reset SQL history/counters. Original sealing key remains required. Consistent stopped pair rollback stays separately supported. |

## Fixed ACTIVE publication and private pool contract

Append0068 adds a distinct STRICT insert-only SQL publication containing the original
64-byte PREPARED checkpoint. Insertion must compare its generation, pair, original
floor, retired-intent cutoff and rebased count against the persisted PREPARED row.
A separate redb publication binds those exact bytes to the exclusively held backend.
Publication order is SQL then redb. Before both publications exist, no runtime store
capability, producer, listener or positive network-ready result is issued.

Only the exact SQL-active/KV-prepared cut may finish initial publication. It must
verify the original empty prepared state and fixed SQL rebase, then publish KV
without clearing or allocating anything. Once both are active, restart validates
their fixed publications, immutable pair identity and known layout, not the old
empty-session/contiguous-rebase assumptions: legitimate runtime writes change those.
Missing, incompatible, malformed, unknown or cross-pair state refuses acceptance.

The actual intermediate implementation lives under `identity_upgrade/active`, wired
with `cfg(test)` until the complete private runtime selects these same functions.
It is real SQL/redb publication and pool-setup code, not a second test-only issuer.
No public ACTIVE constructor, server command, store getter or protocol switch exists.
Inner operations require the existing owned-operation wrapper before future production
selection; calling an inner future is not itself cancellation or shutdown ownership.

ACTIVE publication also initializes a persistent KV runtime high-water witness in
the same durable redb transaction. Startup bounds the complete existing parser scan,
including orphan bindings, and refuses witness/binding floors above the checked SQL
global allocator. It retains the witness even with no sessions. Per-connection setup
checks fixed publications and the bounded authority catalog, not a full session scan.
There is no ordinary request mode/history query. Startup limits are explicit and an
incomplete scan refuses acceptance rather than treating a prefix as complete.

The `epoch/runtime.rs::advance_in` transaction helper is prepared, but existing issue,
promotion, revocation and intent-cleanup transactions **do not select it yet**. All
must advance that witness in their SAME durable KV transaction before any positive
SQL acknowledgment or acknowledged credential/session effect. No-session revocation,
account deletion and cleanup must retain it. This is a required closed-bundle gate,
not an optional optimization. Current fixture coverage of this helper does not prove
that production finalizers update it. Full cutpoint/ACK/consumer tests remain required.

This check cannot prove absence of an unacknowledged SQL-only commit subsequently
rolled back outside the supported protocol, nor detect an arbitrarily restored fully
old pair without an external monotonic witness. Never claim arbitrary partial restore
safety. The explicit consistent restore/reauthentication generation protocol below
remains mandatory, even after all runtime witness writers are selected.

Active protected DML requires a distinct private v3 capability. The v2 stopped
capability must not satisfy it. Each active connection captures the actual accepted
KV owner in its registered callbacks and verifies its SQL publication at connection
creation. Do not expose public active collation registration, raw pool, parsed-marker
constructor or caller readiness boolean. Pool lifetime and SQL worker teardown must
not outlive the actual KV handle. No per-request publication/counter query is added.

Migration0068 installs an empty publication table and 45 additional protected-write
guards. Those trigger checks add real per-row protected-write work even in legacy
mode; SQL work counters do not measure trigger execution cost. Reads are unchanged.
Offline/startup validation and connection setup have additional queries and bounded
catalog/history work. No latency, throughput or whole-platform performance claim has
been measured for this seam.

Within the same binary, active SQL stores and session stores must carry sealed
in-memory mode capabilities. Legacy mutation APIs refuse on active stores rather
than inheriting broad v3 authority. All public aliases remain compatible in legacy
mode. A private pool alone does not enforce this: inspect every method handed that
pool and all standalone recovery/CLI compositions before exposing runtime stores.

## Delivery gates

Internal publication and restart code is an implementation seam, not activation.
Production exposure requires all rows above selected together, actual-store tests at
every publication/commit/cleanup/ACK cut, duplicate-owner and cancellation tests,
cached old writers and same-binary legacy API refusal, all HTTP issuers and settings
flows, explicit backup restore and downgrade fixtures, and representative work-count
and latency checks. Prepared fixtures and static source checks are not those results.
