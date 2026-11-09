# Identity passkeys retirement progress

Progress on the `identity.passkeys` ledger row (WebAuthn ceremonies, credential
management and origin checks). This slice moves the ceremonies and the registration and
assertion decisions into `rbit-identity`. The row stays `baseline-retained` because
removing a passkey and the passkey step of the pending sign-in still run in services.
It builds on the [MFA progress](identity-mfa-progress.md).

## Retired in this slice

| Decision | Before | Now |
| --- | --- | --- |
| Relying party from `server.public_url` (RP ID, origin with default ports removed), creation and request options, registration and assertion verification (ES256, EdDSA, RS256 ≥ 2048 bits, origin, RP ID hash, user presence, counters), CBOR decoding | `rbit-auth/src/webauthn.rs` and `webauthn/` | `rbit_identity::passkeys::webauthn` (`rbit_auth::webauthn` and the crate-root names re-export the same types) |
| Settings-page challenges: 10-minute lifetime, 4 per account and ceremony, 10,000 accounts, single use | `TwoFactorService` | `rbit_identity::passkeys::ChallengeBook` |
| Registration: name rules, single-use challenge, 20-passkey limit, 16 KiB response limit, verification, duplicates, first-factor recovery codes | `Services::add_passkey` | `PasskeyApplication::register` |
| Creation and confirmation options; request options for any challenge | `TwoFactorService` | `PasskeyApplication::{creation_options, confirmation_options, request_options}` |
| Assertions: size limit, credential lookup, signature and counter check, one-use counter advance | `two_factor/verification.rs` | `PasskeyApplication::verify_assertion` over the `PasskeyClaims` port |
| Passkey records | `rbit-storage` | `rbit_identity_models::passkeys::{PasskeyRecord, NewPasskey, PasskeyAdded}` (storage re-exports them) |

`PasskeyRepository` is implemented by `TwoFactorStore` in
`rbit-storage/src/identity_two_factor.rs` with the existing statements. The services
claim adapter implements `PasskeyClaims` over the legacy and inactive original-epoch
counter writes. A new `passkeys` identity feature (`two-factor` plus `p256`,
`ed25519-dalek`, `rsa`, `serde_json`, `thiserror`) carries the ceremonies. `rbit-auth`
and `rbit-services` request it, and `scripts/rewrite/identity-features.mjs` enforces
both lists. Refusal reasons stay out of responses: services logs them exactly as before
(`passkey registration refused`, `passkey sign-in refused`).

## Still in `rbit-services` (next slice)

- Removing a passkey: password plus proof, lookup, removal, audit (`password_operations.rs`).
- The passkey step of the pending sign-in: its challenge slot and `sign_in_options`. This
  moves with the pending sign-in owner listed in the MFA progress.
- The factor listing in `TwoFactorService::status`.

## Boundaries

`scripts/rewrite/passkey-retirement.mjs` runs through `checkIdentityRetirement`. It rejects:

- `RelyingParty` or `WebauthnError` definitions outside `crates/rbit-identity/src/passkeys/`;
- registration verification, assertion credential reads or direct passkey additions in
  production source outside the identity crates and storage;
- passkey limits, challenge lifetimes and registration rejection texts in
  `rbit-services/src`.

`scripts/tests/passkey-retirement.test.mjs` covers excerpts of the retired code, the
allowed owners and re-exports, and the whole tree.

## Evidence

Linux, the repository's quick check at the branch's code tip: rewrite contracts
including the structural type check, workspace-wide formatting and Clippy with
`-D warnings`, licenses and advisories, and the front-end build, all passed. The suites of
the changed packages (`rbit-auth`, `rbit-identity`, `rbit-identity-models`,
`rbit-services`, `rbit-storage`) passed too: 1,405 tests, none skipped. The full workspace
suite and Windows were not run for this slice. Locally, `node scripts/rewrite/check.mjs`
reports zero violations and every retirement script test passes.

New tests: `crates/rbit-identity/tests/passkey_application.rs` (4 cases). They cover the
public-URL requirement, name and challenge checks before anything else, confirmation
options only with passkeys, and oversized or malformed assertions never reaching the
claim. The WebAuthn unit tests (ES256, EdDSA, RS256, origin, RP ID hash, counters) moved
with the code. `scripts/tests/passkey-retirement.test.mjs` has 3 cases. The service
`passkeys` and `two_factor` suites ran unchanged.

## Parity

- Option JSON, verification and counters are the moved code, unchanged. The WebAuthn unit
  tests moved with it, and the service `passkeys` and `two_factor` suites run unchanged.
- Registration and assertion checks keep their order: name, challenge, limit, size,
  relying party, verification, storage for registration; size, relying party, credential
  ID, lookup, signature, claim for assertions. The user-facing texts are reproduced by
  `PasskeyRejection`.
- No schema, migration, route or template changed.
