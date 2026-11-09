# Identity MFA retirement acceptance

Acceptance evidence for the `identity.mfa` ledger row (TOTP enrolment, replay protection
and recovery codes), retired in two slices: the authenticator and recovery-code decisions,
then the pending second sign-in step and password-plus-proof confirmations. It follows
the [identity retirement plan](identity-retirement.md).

## First slice: authenticator and recovery codes

| Decision | Before | Now |
| --- | --- | --- |
| TOTP (RFC 6238, 6 digits, 30 s, one step of skew, constant-time comparison, reported step), base32 and `otpauth://` provisioning | `rbit-auth/src/totp.rs` | `rbit_identity::two_factor::totp` (`rbit_auth::totp` and `rbit_auth::{TotpSecret, RecoveryCode}` re-export the same types) |
| Recovery-code format, parsing and owner-bound digest | `rbit-auth/src/totp.rs` | `rbit_identity::two_factor::totp::RecoveryCode` |
| Enrolment: resume a pending secret, refuse when already set up, confirm with a matching code, issue recovery codes only for the account's first factor | `rbit-services/src/two_factor.rs` | `TwoFactorApplication::{begin_enrolment, cancel_enrolment, confirm}` |
| Typed-code proofs: a recovery code is spent; otherwise an enabled authenticator's code is verified and its time step claimed, so each step works once | `two_factor/verification.rs` | `TwoFactorApplication::verify_code` over the `SecondFactorClaims` port |
| Recovery-code regeneration and its "not enabled" refusal | `two_factor/password_operations.rs` | `TwoFactorApplication::regenerate_recovery_codes` |
| Authenticator record | `rbit-storage` | `rbit_identity_models::two_factor::TwoFactorRecord` (storage re-exports it) |

Ports:

- `TwoFactorRepository` is implemented by `TwoFactorStore` in
  `rbit-storage/src/identity_two_factor.rs`. It is the same sealed-secret SQL, and each
  write keeps its guard (confirm only the pending secret, claim only a later step, spend
  only an unused code).
- `SecondFactorClaims` is implemented in services by a small adapter over the existing
  claim modes: legacy claims, and the inactive original-epoch claims, which keep their
  `Invalidated` to `NotFound` mapping.

A new `two-factor` identity feature (`session-tokens` plus `hmac`, `sha1`, `subtle`)
carries the algorithm. `rbit-auth` now requests exactly `session-tokens` and
`two-factor`, and `rbit-services` requests `two-factor` as well. Both expectations are
enforced by `scripts/rewrite/identity-features.mjs`. `Cargo.lock` gains only the two new
optional edges of `rbit-identity`.

## Second slice: the pending sign-in and confirmations

| Decision | Before | Now |
| --- | --- | --- |
| Pending second step owner: bounded map (10,000), 5-minute lifetime, 5 wrong factors, one passkey challenge slot, legacy and receipted entries that never complete each other | `rbit-services/src/two_factor/pending.rs` | `rbit_identity::second_factor::PendingSignIns` |
| The second step: pending lookup, mode check, per-account throttle check, authoritative lookup (active user, or active at the receipt's epoch), single-use passkey challenge, proof, one-time claim of the entry, binding the receipt, throttle reset; wrong factors counted and the entry charged; invalidated receipted entries ended | `two_factor/sign_in.rs` | `SecondFactorStep::complete` |
| Password-plus-proof confirmation: throttle check, admission before any proof, confirm-challenge taken first, password then proof, failure counted | `two_factor/password_checks.rs` | `SecondFactorStep::confirm` |
| Removing the authenticator and removing a passkey (lookup first, then confirmation, then removal) | `two_factor/password_operations.rs` | `SecondFactorStep::{disable_authenticator, remove_passkey}` |
| Proof types (`SecondFactorProof`, `SecondFactor`) | `rbit-services` | `rbit_identity::second_factor` (services re-exports them) |

The claim ports now return the guarded write's outcome (`Claimed`, `Rejected`,
`Invalidated`) instead of a boolean. The services claim adapter
(`two_factor/verification.rs`) selects the existing legacy writes, or the inactive
original-epoch writes for a receipted entry, without changing either. A new
`SecondStepAccounts` port is implemented by `AccountStore` with the existing active-user,
fenced epoch and login lookups. Throttle buckets (`TwoFactor`, per account) and audit
entries stay in services, with the same scopes, keys, actions and details.

Nothing is activated. Receipted entries remain reachable only through the existing
inactive `complete_original_sign_in`, `start_original_password_pending` and provider
seams. No route selects them, and session storage is unchanged, as
[identity retirement](identity-retirement.md) requires.

Left in services as composition: the Git HTTP password gate (`authenticate_git_password`,
first-factor throttles, and "token required" when any factor exists), the settings factor
listing (`status`), and the confirm throttle around enrolment. None of these is a
second-factor decision.

## Boundaries

`scripts/rewrite/mfa-retirement.mjs` runs through `checkIdentityRetirement`. It rejects:

- `TotpSecret` or `RecoveryCode` definitions outside `crates/rbit-identity/src/two_factor/`;
- generating, restoring or parsing them in production source outside the identity crates
  and storage;
- enrolment or recovery-code writes on the two-factor store outside identity and storage;
- the code count or the enrolment rejection texts in `rbit-services/src`.

`scripts/tests/mfa-retirement.test.mjs` covers excerpts of the retired code, the allowed
owners, tests and re-exports, and the whole tree.

`scripts/rewrite/second-factor-retirement.mjs`, in the same chain, rejects pending-owner,
proof-type or step definitions outside `crates/rbit-identity/src/second_factor*`. It also
rejects one-use claim writes in production source outside identity, storage and the
services claim adapter, and pending lifetime or attempt-limit constants in services.
`scripts/tests/second-factor-retirement.test.mjs` covers it.

## Evidence

Linux, the repository's quick check at the branch's code tip: rewrite contracts
including the structural type check, workspace-wide formatting and Clippy with
`-D warnings`, licenses and advisories, and the front-end build, all passed. The suites of
the changed packages (`rbit-auth`, `rbit-identity`, `rbit-identity-models`,
`rbit-services`, `rbit-storage`) passed too: 1,405 tests, none skipped. The full workspace
suite and Windows were not run for this slice. Locally, `node scripts/rewrite/check.mjs`
reports zero violations and every retirement script test passes.

New tests: `crates/rbit-identity/tests/two_factor_application.rs` (4 cases). They cover
resumed enrolment and first-factor-only codes, a second factor keeping existing codes,
step replay and older-step refusal, spent recovery codes, and regeneration needing a
factor. The TOTP unit tests (including the RFC 6238 vectors) moved with the algorithm.
`scripts/tests/mfa-retirement.test.mjs` has 3 cases. The service `two_factor` and
`identity_races` suites ran unchanged.


Second slice: `crates/rbit-identity/tests/second_factor_step.rs` (6 cases) drives the
moved step with in-memory ports. It covers:

- two proofs for one pending step released together after the lookup, with exactly one
  completing it;
- a step cancelled after the lookup, which is neither completed nor counted;
- wrong factors counted until the entry is dropped;
- suspension found by the lookup ending the step, and receipted mode never adopting a
  legacy entry;
- completion resetting the throttle and reporting the first-step method;
- confirmations that verify the password before any proof is spent.

The services `identity_races/second_factor` suite, the private post-lookup barrier tests
(`two_factor/sign_in/tests.rs`: reset, suspension and pending changes after the lookup
cannot consume an original factor) and `two_factor/pending/tests.rs` run unchanged against
the delegating adapter. The `two_factor`, `passkeys` and `sessions` suites also run
unchanged.

Second slice, Linux, the repository's quick check at the code tip: rewrite contracts
including the structural type check, workspace-wide formatting and Clippy with
`-D warnings`, licenses and advisories, and the front-end build, all passed. The suites of
the changed packages (`rbit-identity`, `rbit-identity-models`, `rbit-services`,
`rbit-storage`) passed too: 1,391 tests, none skipped. This includes the existing
`identity_races`, `sessions`, `two_factor`, `passkeys`, `password_admission`, pending
and post-lookup barrier suites, unchanged, plus the new `second_factor_step` race tests.
The full workspace suite and Windows were not run.

## Parity

- Rejection texts are reproduced byte for byte by `TwoFactorRejection` and asserted in the
  identity tests. The audit entry for enabling the authenticator is still written by
  services, after the same confirmation.
- Typed-code proofs keep their order (recovery-code parse first, then the enabled
  authenticator), the same claim writes, and the same `InvalidCredentials` for every
  refusal.
- No schema, migration, route or template changed; 0073 remains the latest migration.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | First slice 1, second slice 1, plus a boundary commit and a record commit per slice |
| Second slice: files changed under `crates/` | 20 |
| Second slice: source (moved owner and step included) | 17 files, +827 / −422 |
| Second slice: tests | 3 files, +318 / −9 |
| Boundary | one new 38-line guard, chained into the identity check |

Tricky parts:

- Existing suites drive private owner seams (`lock_pending`, `complete_second_factor_after`,
  `finish_pending`). Services keeps them as thin, test-only delegations to the identity
  owner, so the suites run unchanged.
- The inactive original-epoch claims returned `ServiceError::NotFound` for an invalidated
  credential. The claim ports now carry the write outcome (`Invalidated`) instead, so the
  step ends a receipted entry without depending on service errors.
- A precedence test requires that the proof is converted only after the passkey lookup.
  Removal therefore takes `impl Into<SecondFactorProof>` and converts late.
- Clippy rejected the large error variant (the refused `User`, now boxed) and dead
  test-only seams (now `cfg(test)`).

Estimate for similar rows: a stateful, race-sensitive owner with inactive seams took about
three hours of work plus queueing. Most of it went into preserving private test seams and
exact error precedence, not into the move itself.
