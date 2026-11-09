# Identity recovery retirement acceptance

Acceptance evidence for the `identity.recovery` ledger row: one-use password reset,
verified email and enumeration resistance. It follows the
[identity retirement plan](identity-retirement.md) and the
[profile retirement](identity-profile-acceptance.md), which landed in the same branch.

## What moved

| Decision | Before | Now |
| --- | --- | --- |
| Reset requests: email availability, client throttle, address parsing, recipient throttle, then an outcome that callers answer identically (send, suspended, unknown) | `rbit-services/src/recovery.rs` | `rbit_identity::recovery::RecoveryLinks::request_reset` |
| Link issuance: 256 random bits encoded like a session token, only the digest stored, bound to purpose, current address and lifetime (1 hour reset, 2 days verification) | `issue_token` in services | `RecoveryLinks::issue` |
| Reset link preview: honoured now, account exists, not suspended (`Forbidden`) | `reset_account` in services | `RecoveryLinks::reset_account` |
| Verification: single-use consume, mark verified only while the address is unchanged, retire sibling links | `RecoveryService::verify_email` | `RecoveryLinks::verify_email` |
| Resend, email status, verified-among (administrators), administrator reset target (suspension rejection) | services | `RecoveryLinks` |
| When sign-up or an address change needs a verification link | `email_events.rs` | `RecoveryLinks::{sign_up_verification, changed_email_verification}` |
| Replacement passwords: reservation timing, link check, sign-up length rules, initial-password must change, admission, hashing; administrator set-password (administrators only, never their own) | `recovery/password_changes.rs` | `RecoveryPasswords::{prepare_reset, prepare_admin_password}` |
| Single-use redemption and replacement commits | direct store calls | `commit_reset`, `commit_admin_password` |
| Link records (`TokenPurpose`, `NewEmailToken`, `EmailTokenRecord`, `RecoveryAccount`) | `rbit-storage` | `rbit_identity_models::recovery`; storage re-exports the same types |

Ports: `RecoveryRepository` is implemented by `RecoveryStore`. `RecoveryCredentialRepository`
is implemented by `rbit_storage::RecoveryCredentials`, which wraps the account and profile
stores. Both live in `rbit-storage/src/identity_recovery.rs` and keep the existing SQL.
Single use is still enforced by the database:

- `consume` and `reset_password` each mark the link used in one guarded statement or
  transaction, so of two concurrent redemptions at most one succeeds.
- A link is honoured only while it is unused, unexpired and addressed to the account's
  current email.

`RecoveryService` keeps its API and constants. It renders and queues mail, logs, audits,
publishes `PasswordChanged`, and runs each commit followed by session finalization in the
tracked `finish_update` worker, as before. Its mail-request throttle trait
(`RequestThrottle`) is unchanged. A small adapter maps it to identity's `RecoveryThrottle`.
Event-driven verification mail is unthrottled, as before.

## Legacy code removed

1. `issue_token`, `parse_token`, `recipient_key`, `validate_password`, `require_admin`,
   the lifetime and length constants, and their unit tests in
   `rbit-services/src/recovery.rs`. The tests moved into `rbit-identity`.
2. Direct link-store calls, admission and hashing in `recovery/password_changes.rs` and
   `recovery/email_events.rs`.
3. The duplicate record definitions in `rbit-storage/src/sqlite/recovery.rs`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/recovery-retirement.mjs` runs through `checkIdentityRetirement`. It rejects:

- `NewEmailToken` construction, or `find_valid`, `mark_verified`,
  `find_account_by_email` or purpose-bound `consume` calls, in production source outside
  the identity crates and storage;
- admission or hashing (`passwords.begin`, `hash_if_different`, `attempt.hash`) in the
  service recovery adapter;
- recovery lifetime constant definitions or recovery rejection texts in `rbit-services/src`.

`scripts/tests/recovery-retirement.test.mjs` covers excerpts of the retired code, the
allowed owners, tests, other services and re-exports, and the whole current tree.

## Results

Linux, at the branch tip built on the integrated stack: the repository's full check
(rewrite contracts including the structural type check, workspace-wide formatting and
Clippy with `-D warnings`, licenses and advisories, the front-end build, and the full
nextest suite) passed. Windows suites were not run for this slice. Locally:
`node scripts/rewrite/check.mjs` reports zero violations and the retirement script tests
(token, profile, recovery and identity chains) pass.

New tests, all in the passing set:

- `crates/rbit-identity/tests/recovery_application.rs` (6 cases): client-then-recipient
  throttling and identical outcomes for known, unknown and suspended addresses; a digest-only
  link bound to purpose, address and lifetime; single-use verification that retires sibling
  links; invalid passwords leaving the link usable, reservation precedence and one winning
  redemption out of two; the initial-password and suspension rules; administrator-only
  set-password that refuses the administrator's own account.
- `crates/rbit-identity/src/recovery.rs` and `recovery/passwords.rs`: the original unit
  tests (malformed links, recipient keys, password rules), moved.
- `scripts/tests/recovery-retirement.test.mjs` (3 cases).

`crates/rbit-services/tests/recovery.rs`, the storage `recovery` and `identity_races`
suites, and the web password-protocol suites ran unchanged.

## Parity argument

- Routes, templates, mail messages, catalog text and status codes are untouched. No
  `rbit-web` or `rbit-mail` file changed.
- Enumeration resistance: the request path performs the same throttles and lookup and
  returns `Ok(())` for every outcome. Only the log line differs by outcome, as before.
- Order of effects:
  - Reset: reservation (before the first await), link lookup, account, suspension, length
    rules, initial-password flag, admission, then the deferred reservation error, hashing
    (or hash-if-different), the single-use transaction, and session finalization.
  - Administrator set-password: administrator check, reservation, account, own-account
    check, length rules, admission, reservation error, hashing, commit, finalization.
- Error text: every rejection string is reproduced by `RecoveryRejection`'s `Display`
  (asserted in the identity tests), including `EMAIL_UNAVAILABLE`, still exported from
  `rbit_services::recovery`.

## Migration, rollback and performance

No migration, schema or stored format changed; 0073 remains the latest migration. Rollback
is the previous binary against the same data. Each request performs the same storage calls
and password work in the same order. No release measurement was taken and none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 19 |
| Lines added / removed (code commits) | 1,165 / 228 |
| Rust under `crates/*/src` (moved unit tests included) | 14 files, +757 / −227 |
| Tests (`crates/*/tests`, `scripts/tests`) | 2 files, +365 |
| Checker and manifests | 3 files, +43 / −1; one new 39-line guard chained into the identity check |

Tricky parts:

- Password state spans three stores (link store, accounts, logins). A second port,
  implemented by a small `RecoveryCredentials` wrapper, avoids moving SQL between stores.
- Event-driven verification mail is unthrottled while user requests are throttled. The
  same `RecoveryLinks` serves both with a no-op throttle for events.
- Single use is a database property (a guarded consume and one reset transaction). The
  application never pre-checks in a way that could race, and the new tests model two
  prepared resets with one winner.

Estimate: profile and recovery together took about four hours of work in one branch,
much of it waiting for build capacity. The three remaining identity rows (`mfa`,
`passkeys`, `external`) are similar in shape: throttle order, admitted work and
finalization-ordered commits. Expect three to four code commits and roughly half a day
each. `passkeys` is likely the largest, because WebAuthn state lives partly in `rbit-auth`.
