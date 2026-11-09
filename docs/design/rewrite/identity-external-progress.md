# Identity external sign-in retirement progress

Progress on the `identity.external` ledger row (OIDC and GitHub sign-in, PKCE, linking and
verified identity). This slice moves only the protocol building blocks into
`rbit-identity`. The row stays `baseline-retained`: the external sign-in decisions in
`rbit-services/src/external_auth*` (about 2,200 lines) are still there. They are not
retired here.

## Retired in this slice

| Decision | Before | Now |
| --- | --- | --- |
| Flow cookie sealing (HMAC-SHA256, expiry with clock leeway), state comparison, PKCE `S256`, nonce and link-token derivation | `rbit-auth/src/oidc/flow.rs` | `rbit_identity::oidc` |
| ID token validation: compact JWS with RS256 (≥ 2048 bits) or ES256 only, key by `kid` with `UnknownKey` for rotation, `iss`, `aud`/`azp`, `exp`, `iat`, `nbf`, `nonce` | `rbit-auth/src/oidc/tokens.rs` | `rbit_identity::oidc` |

The moved code and its tests are unchanged apart from the random-bytes import. `rbit_auth::oidc`
and the crate-root names re-export the same types. A new `external` identity feature
(`session-tokens` plus `hmac`, `subtle`, `p256`, `rsa`, `serde_json`, `thiserror`)
carries it; `rbit-auth` and `rbit-services` request it, and
`scripts/rewrite/identity-features.mjs` enforces both lists. `Cargo.lock` is unchanged.

## Still in `rbit-services` (next slices)

Account and provider-login decisions already run in
`rbit_identity::accounts::{ProviderAuthentication, ProviderOrigin}`. Still to move:

- Starting and finishing a flow: provider choice, `next` sanitising, cookie lifetime, the
  link-token check, callback state and PKCE checks (`external_auth.rs`, `flows.rs`,
  `callback_mode.rs`).
- Choosing what a verified identity does: sign in a linked account, link to the signed-in
  account, create an account under the registration policy, or refuse an unverified email
  (`external_auth.rs`, `links*`).
- The inactive original-proof provider path (`original.rs`). It should move with the
  original-proof session protocol.
- The provider HTTP client and discovery (`client.rs`). This is infrastructure, so it
  should stay an adapter behind a port.

## Boundaries

`scripts/rewrite/oidc-retirement.mjs` runs through `checkIdentityRetirement`. It rejects
any `FlowKey`, `Pkce`, `JwkSet`, `IdTokenCheck` or `OidcError` definition, or
`verify_id_token` function, outside `crates/rbit-identity/src/oidc*`.
`scripts/tests/oidc-retirement.test.mjs` covers the retired definitions, allowed uses and
the whole tree.

## Evidence

Linux, the repository's quick check at the branch's code tip: rewrite contracts
including the structural type check, workspace-wide formatting and Clippy with
`-D warnings`, licenses and advisories, and the front-end build, all passed. The suites of
the changed packages (`rbit-auth`, `rbit-identity`, `rbit-identity-models`,
`rbit-services`, `rbit-storage`) passed too: 1,405 tests, none skipped. The full workspace
suite and Windows were not run for this slice. Locally, `node scripts/rewrite/check.mjs`
reports zero violations and every retirement script test passes.

The OIDC unit tests (flow sealing, PKCE, ID-token algorithms, claims and key rotation)
moved with the code. `scripts/tests/oidc-retirement.test.mjs` has 3 cases. The service
`external_auth` suites ran unchanged.
