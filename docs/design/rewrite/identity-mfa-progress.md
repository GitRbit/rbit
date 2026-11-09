# Identity MFA retirement progress

Progress on the `identity.mfa` ledger row (TOTP enrolment, replay protection and recovery
codes). This slice retires the authenticator and recovery-code decisions. The row stays
`baseline-retained` because the pending second sign-in step still lives in
`rbit-services`. It follows the [identity retirement plan](identity-retirement.md).

## Retired in this slice

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

## Still in `rbit-services` (next slice)

- The pending sign-in owner (`two_factor/pending.rs`): bounded in-memory map, TTL,
  per-pending attempt limit, passkey challenge slot. It also holds the inactive
  original-proof receipts.
- The second sign-in step (`two_factor/sign_in.rs`, `password_sign_in.rs`): throttle,
  account lookup, the one-time claim of the pending entry, and audit wording per factor.
- Password-plus-proof checks for sensitive changes (`password_checks.rs`) and the Git
  HTTP password gate for accounts with two-factor authentication.
- The `TwoFactor` throttle around confirmation.

These are coupled to the inactive original-proof session protocol, so they should move
together with it rather than piecemeal. Passkey proofs within the same verifier belong to
`identity.passkeys`.

## Boundaries

`scripts/rewrite/mfa-retirement.mjs` runs through `checkIdentityRetirement`. It rejects:

- `TotpSecret` or `RecoveryCode` definitions outside `crates/rbit-identity/src/two_factor/`;
- generating, restoring or parsing them in production source outside the identity crates
  and storage;
- enrolment or recovery-code writes on the two-factor store outside identity and storage;
- the code count or the enrolment rejection texts in `rbit-services/src`.

`scripts/tests/mfa-retirement.test.mjs` covers excerpts of the retired code, the allowed
owners, tests and re-exports, and the whole tree.

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

## Parity

- Rejection texts are reproduced byte for byte by `TwoFactorRejection` and asserted in the
  identity tests. The audit entry for enabling the authenticator is still written by
  services, after the same confirmation.
- Typed-code proofs keep their order (recovery-code parse first, then the enabled
  authenticator), the same claim writes, and the same `InvalidCredentials` for every
  refusal.
- No schema, migration, route or template changed; 0073 remains the latest migration.
