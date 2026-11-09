# Identity tokens retirement acceptance

Acceptance evidence for the `identity.tokens` ledger row: personal access token scopes,
expiry, hashed secrets and revocation. It follows the
[identity retirement plan](identity-retirement.md) and its
[acceptance record](identity-retirement-acceptance.md). The remaining identity rows
(`profile`, `recovery`, `mfa`, `passkeys`, `external`) stay `baseline-retained`.

## What moved

| Decision | Before | Now |
| --- | --- | --- |
| Name trimming and the 64-character limit, the offered expiry periods, expiry arithmetic | `rbit-services/src/tokens.rs` | `rbit_identity::access_tokens::AccessTokenApplication::create` |
| Secret generation; storing only the SHA-256 digest and a four-character hint | `rbit-services`, `rbit-auth::access_token` | `rbit_identity::access_tokens`, `rbit_identity::token::AccessTokenSecret` |
| The 50-token per-account limit and its message | `rbit-services` | `rbit_identity::access_tokens` (the store still enforces it atomically in its `INSERT … WHERE COUNT < limit`) |
| Owner-scoped listing and revocation, `NotFound` for another account's token | `rbit-services` | `AccessTokenApplication::{list, revoke}` |
| Credential resolution: format (`rbit_` and legacy `rit_`), digest lookup, expiry, required password change, suspension, last-use recording that never refuses a valid token | `rbit-services` | `AccessTokenApplication::authenticate` |
| Token record (`AccessTokenId`, metadata) and new-token input | `rbit-core`, `rbit-storage` | `rbit_identity_models::access_tokens::{AccessTokenId, AccessTokenRecord<Scope>, NewAccessTokenRecord<Scope>}` |
| Persistence and owner checks | concrete `AccessTokenStore`/`AccountStore` calls | ports `AccessTokenRepository` and `AccessTokenOwnerRepository`, implemented in `rbit-storage/src/identity_access_tokens.rs` by the same SQL |

The scope is a type parameter of the identity record. `TokenScope` and its `limit`/`allows`
semantics stay in `rbit-core` because capping repository access is authorization policy
(`RepoAccess` lives in `rbit-access`, which `rbit-identity-models` may not depend on).
Identity stores and returns the scope without interpreting it. `rbit_core::AccessToken`
is now `AccessTokenRecord<TokenScope>`, `rbit_core::AccessTokenId` and
`rbit_auth::AccessTokenSecret` are re-exports, and `rbit_storage::NewAccessToken` is
`NewAccessTokenRecord<TokenScope>`: the same types at the old paths, so callers and
struct literals compile unchanged.

`rbit-services::AccessTokenService` keeps its public API (including
`EXPIRY_CHOICES`, now delegated) and is only the composition adapter: it maps
`AccessTokenError` to `ServiceError` (rejection text through `Display`, byte-identical to
the old strings), logs, and writes the `TokenCreated`/`TokenRevoked` audit entries. The new
`access-tokens` identity feature adds no dependency beyond `session-tokens`.

## Legacy code removed

1. The policy body of `rbit-services/src/tokens.rs`: validation constants, secret
   handling, expiry arithmetic, the per-account limit and all direct store calls.
2. The `AccessTokenSecret` implementation in `rbit-auth/src/access_token.rs`; the file is
   now a re-export with the original unit tests plus a type-identity test.
3. The duplicate `AccessToken`/`AccessTokenId` definitions in `rbit-core` and
   `NewAccessToken` in `rbit-storage`.

## Boundaries that fail if legacy paths return

`scripts/rewrite/token-retirement.mjs`, run through `checkIdentityRetirement` by
`scripts/rewrite/check.mjs`, rejects:

- a `struct`/`enum`/`type AccessTokenSecret` anywhere below `crates/` except
  `crates/rbit-identity/src/token/`;
- any `AccessTokenSecret` use in `crates/rbit-services/src/`;
- a literal token limit or expiry list (`MAX_TOKENS_PER_*`, `EXPIRY_CHOICES = &[…]`) in
  production source outside the identity crates and storage;
- direct `create_with_limit`/`find_by_digest` calls (or `AccessTokenStore::` token
  operations) in production source outside the identity crates and storage.

`scripts/rewrite/identity-features.mjs` now requires `rbit-services` to request exactly
`access-tokens`, `password-work` and `sessions`, and checks that the `access-tokens` graph
stays crypto-only (no `argon2`, `tokio`, `zeroize`, `rbit-kernel` or `subtle`).
`scripts/tests/token-retirement.test.mjs` covers each rule, the allowed delegating
constant, comments, and the whole current tree.

## Results

All on Windows through the shared workspace helper, at the code commits of this slice.
Full workspace suites and Linux CI were not run for this slice.

| Gate | Command scope | Result |
| --- | --- | --- |
| Compile | `check --all-targets` for `rbit-identity-models`, `rbit-identity`, `rbit-core`, `rbit-auth`, `rbit-storage`, `rbit-services`, `rbit-web` | Pass, no warnings |
| Clippy `-D warnings` | `--all-targets` for the same crates except the `rbit-services` integration target; `rbit-services` lib | Pass |
| Clippy, `rbit-services` integration target | `--all-targets` | 16 `large_futures` errors, all in `tests/pulls.rs` (`Fixture::new`, 16,424 bytes), a file and fixture this slice does not touch; not verified against the baseline on this host |
| `rbit-identity` `access_tokens` (new) | 7 tests | Pass |
| `rbit-identity` `session_tokens`, `model_compatibility` | 4 tests | Pass |
| `rbit-auth` lib (access token vectors kept verbatim, plus type identity) | 45 tests | Pass |
| `rbit-core` lib (scope tests, plus record type identity) | 142 tests | Pass |
| `rbit-identity-models` lib | 16 tests | Pass |
| `rbit-storage` integration: `access_tokens::`, `upgrade::` | 26 tests | Pass |
| `rbit-services` integration: `tokens::` (includes `actions_tokens::`), `audit::`, `admin::`, `password_admission::`, `permission_matrix::` | 56 tests | Pass |
| Rewrite policy (`node scripts/rewrite/check.mjs`) | whole tree | 0 violations |
| `cargo fmt --all --check` | whole tree | Pass |
| Node script tests (`node --test scripts/tests/*.test.mjs`) | 651 | 642 pass, 8 skipped, 1 failure: `htmx-vendor.test.mjs` needs `node_modules` (unrelated, as in the identity record) |

New tests: `crates/rbit-identity/tests/access_tokens.rs` (issuance stores only digest and
hint; legacy rejection texts verbatim and no storage call before validation; per-account
limit; owner-scoped revocation; both prefixes accepted with last use recorded; check
order for password change and suspension; expiry and failed bookkeeping),
`scripts/tests/token-retirement.test.mjs` (5 cases), the updated
`identity-features.test.mjs` case, and type-identity tests in `rbit-core` and `rbit-auth`.
`rbit-services/tests/tokens.rs` and every other existing suite ran unchanged.

Not run here: `rbit-web` integration suites that use tokens (`password_protocols`,
`permission_matrix`, `actions_*`, `hostile_downloads`), `rbit-e2e`, and the Linux CI jobs.
They compile (all-target check and Clippy pass) and use only the unchanged service API.

## Parity argument

- Routes, status codes, cookies, CSRF, rate limits and templates are untouched: no
  `rbit-web` file changed. Git HTTP still routes credentials with
  `rbit_auth::AccessTokenSecret::looks_like_token`, the same type.
- Error text: the four rejection strings are reproduced by `AccessTokenRejection`'s
  `Display` and asserted verbatim in the new identity tests; services wraps them in the
  same `ServiceError::Invalid`. Unknown or foreign tokens remain `ServiceError::NotFound`.
- Check order in `authenticate` is unchanged (format, lookup, expiry, password change,
  suspension, then last use) and is pinned by call-order assertions. A suspended owner is
  still logged and refused; a failed last-use write is still logged and accepted.
- Secrets: same 256-bit CSPRNG bytes, same base64url encoding and `rbit_` prefix, same
  digest over the random part only, same hint; the `rbit-auth` vector tests pass through
  the re-export.
- Stored data: the same SQL statements run through thin adapters; the token record type
  is field-for-field identical.

## Migration, rollback and performance

No migration, schema, table or cookie format changed; migration 0073 remains the latest
and no historical migration was edited. Rollback is the previous binary against the same
data. Each request path performs the same storage calls in the same order; the only added
work is constructing a borrowed application value. No release measurement was taken and
none is claimed.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 24 |
| Lines added / removed (code commits) | 809 / 180 |
| Rust under `crates/*/src` (moved unit tests included) | 16 files, +444 / −165 |
| Tests (`crates/*/tests`, `scripts/tests`) | 3 files, +305 / −4 |
| Checker, features and manifests | 5 files, +60 / −11; one new 39-line Node guard |
| Elapsed | About three hours, roughly half of it queued behind the shared build lease |

Tricky parts:

- `TokenScope::limit` caps `RepoAccess`, which lives in `rbit-access`; identity models may
  not depend on it, and existing tests call `scope.limit(...)` as an inherent method. Making
  the identity record generic over the scope kept scope semantics in `rbit-core` and every
  call site and struct literal unchanged.
- The log lines (`suspended user's access token refused`, `failed to record access token
  use`) are part of observable behaviour, but `rbit-identity` has no tracing. The
  application returns a `TokenAuthentication` outcome carrying the suspended user and the
  bookkeeping result, so services logs exactly as before.
- Lint differences surfaced only in Clippy (type complexity on the port, manual async fn
  in the adapter).

Estimate for similar rows: a row whose legacy service is a small, self-contained policy
over one or two stores (like this one, about 130 lines) takes 3 code commits and half a
day of work plus build-lane waiting. Rows with several stores, cross-row models or
background effects (`identity.recovery`, `identity.mfa`, `identity.external`) should be
expected to take 2–4 times as long, mainly in port design and parity tests.
