# Identity external sign-in acceptance

Acceptance evidence for the `identity.external` ledger row ("OIDC/GitHub sign-in, PKCE,
linking and verified identity"), target `rbit-identity`, following the
[identity retirement](identity-retirement.md) pattern. The first slice
([progress record](identity-external-progress.md)) moved the protocol building blocks
(flow sealing, PKCE, nonce and link tokens, ID-token validation) into
`rbit_identity::oidc`. This slice moves the remaining decisions and retires the row.

## What moved

New module `rbit_identity::external` (feature `external`, which `rbit-services` already
requests; no feature list, dependency or lockfile change):

| Decision | Legacy place | Now |
| --- | --- | --- |
| Callback checks before any exchange: flow cookie opens within `FLOW_TTL_SECONDS`, is for this provider, `state` matches exactly, provider `error` reported (`access_denied` wording, other errors truncated to 120 characters without control characters), code present and ≤ 2048 bytes; log reasons and user text | `ExternalAuthService::verify_callback` | `external::callback::check`, `callback::unconfirmed` |
| Authorization request: code flow, client, redirect, scopes, `state`, PKCE `S256` challenge, OIDC nonce | `authorization_flow` | `external::provider::authorization_query` |
| Verified identity from the ID token (`amr` contains `mfa`), userinfo only without an address and only for the same `sub` (`email_verified` as boolean or case-insensitive `"true"`), GitHub id/login/name and verified primary address | `oidc_identity`, `github_identity` | `external::provider::{oidc_identity, needs_userinfo, apply_userinfo, github_identity}` |
| What a verified identity does: linked → link request for the same account only, else touch and sign in; link by a signed-in active account; link by a trusted verified address; register when allowed; refuse with explanation | `Services::complete_external_with` | `external::completion::complete` over the `ExternalAccounts` port |
| Signing in: suspended → audited refusal; second step unless `trust_mfa` and `mfa`; audit text including `(provider MFA)` | `finish_external_with` | `completion::sign_in_linked` |
| Registration allowed: never before setup; `Never`/`FollowSite`/`Always` | `external_registration_allowed`, `may_register` | `completion::registration_allowed`, `external::may_register` |
| "Choose a username" step: username suggestion, verified-address flag, create-time checks and texts, ending the step, audit order | `registration_draft`, `register_external_with`, `suggest_username` | `external::{suggested_username, vouched_email}`, `registration::register` over `ExternalRegistrations` |
| Pending steps: 15-minute TTL, 10,000 cap dropping the oldest, token digests only | `ExternalAuthService::{start_registration, pending_registration, end_registration}` | `external::pending::PendingRegistrations` |
| Last way to sign in and its refusal text | `SignInMethods::is_last_method`, `unlink` | `external::{is_last_method, LAST_METHOD}` |
| Link audit detail, link refusal texts | `linked_entry`, `links.rs` copies | `external::linked_detail`, `completion::{already_linked, link_refused}` |

`rbit-services/src/external_auth*` keeps the adapters:

- the provider HTTP client with its outbound address guard, discovery and key caches,
  token exchanges and response parsing (`client.rs`, unchanged), and the flow key kept in
  `server_secrets`;
- `flows.rs`: one `ModeAccounts` adapter per request implementing both ports over the
  external-identity and account stores, audit, two-factor and events, for the legacy and
  the inactive fixed-origin modes (`original.rs`, whose `Mode` lookups it calls);
- the settings listing (`methods`), which reads the stores and configuration and
  delegates `is_last_method`; the store's unlink SQL still enforces the last-method rule.

`VerifiedIdentity`, `CallbackParams`, `FLOW_TTL_SECONDS` and `REGISTRATION_TTL` are
re-exported from `rbit_services::external_auth`, so every caller and suite compiles
unchanged. The outbound address guard, PKCE, nonce and link-token handling are
unchanged code; linking rules, the order of store calls and every message and audit text
are moved verbatim.

### Not legacy, not moved

The inactive original-proof provider path (`original.rs`, `links*`,
`callback_original`, `register_external_original`, `complete_external_sign_in_original`)
is not production code. Like the inactive epoch session paths in the
[identity retirement](identity-retirement.md#scope), it moves with the original-proof
session protocol. It already runs the same `rbit_identity::external` completion and
registration decisions through its own `Mode`; its manual-link path now uses the shared
link texts and audit detail.

Excluded by the coordinator's split: the second sign-in step itself (`two_factor`,
sessions, passkeys). The completion port only asks whether two-factor is enabled and
starts the pending step through the existing service.

## Test edits

None to existing suites. The inline `external_auth` unit tests still call
`suggest_username` (imported for tests from identity) and `SignInMethods::is_last_method`
(now delegating). `rbit-services` `tests/external_auth.rs` and `tests/provider_original.rs`
and the `rbit-web` external sign-in routes compile against the re-exports.

New tests in `rbit-identity`: `external::tests` (every completion branch with the exact
call and audit order, registration outcomes and texts, callback refusals, username
suggestion, registration policy, last method), `external::provider::tests` and
`external::pending::tests`.

## Boundary that fails if legacy paths return

`scripts/rewrite/external-sign-in-retirement.mjs`, with its own import and call in
`scripts/rewrite/check.mjs`, over production Rust (comments and a trailing inline test
module ignored):

- in `rbit-services/src/external_auth*` and `rbit-auth/src`: a `VerifiedIdentity`,
  `CallbackParams`, `Refusal`, `ProviderPolicy`, `Completion` or `ProviderRegistration`
  definition, or a `suggest_username`, `may_register`, `linked_entry` or `truncate`
  function;
- outside `rbit-identity/src/external*`: the refusal and audit texts;
- in `rbit-services/src/external_auth*`: `state_matches`, opening the flow cookie,
  `trust_mfa &&`, `email_verified &&`, the registration match, `code_challenge_method`,
  `amr`, reading `email_verified` from userinfo, `MAX_PENDING_REGISTRATIONS`,
  `lock_pending` and the last-method text.

`scripts/tests/external-sign-in-retirement.test.mjs` covers each rule, the allowed places
(identity, configuration, two-factor, suites, comments), the current tree and the base
files at `08dafe3a`.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same store calls in the same order with the same short-circuits (one timestamp per request for touch/link/link-by-email, as before); same errors (`Invalid` text, `Forbidden`, `NotFound`, `RegistrationClosed`, name errors, store errors). Pinned by the new recording-port tests and by the unchanged `external_auth` and `provider_original` suites. |
| Security | Flow cookie, `state`, PKCE, nonce, link token, ID-token checks and the outbound address guard are unchanged; an unverified address still links nothing; suspension and the second step gate every sign-in; callback throttling and failure audit unchanged. |
| Migration and rollback | No schema, migration or stored-data change; 0073 remains the latest. Rollback is the previous binary. |
| Performance | No added queries; one borrowed adapter per request. Pending steps keep the same in-memory map. No release measurement claimed. |

## Results

Linux, the repository's quick check (workspace-wide formatting and Clippy with
`-D warnings`, licenses and advisories, front-end build, and the suites of the changed
packages `rbit-identity` and `rbit-services` and their dependants): 935 tests, all
passed, none skipped. That included the unchanged `rbit-services` `external_auth`
(callback refusals, linking, trusted email and MFA, registration and its throttle,
unlink) and `provider_original` suites. The rewrite contracts needed one more pass: the
component gate still required the removed `original::Completion`. Its entry now names
the new adapter `flows::ModeAccounts`, and the structural type check passes locally.
The full workspace suite and Windows were not run for this slice. Locally,
`node scripts/rewrite/check.mjs` reports zero violations and the retirement script test
passes (4 cases).

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 17 |
| Lines added / removed | 1,547 / 350 |
| `rbit-identity` production | 7 files, +744 (six new modules) |
| `rbit-services` production | 4 files, +238 / −349 (`flows.rs` rewritten as the adapter) |
| Tests | 3 new `rbit-identity` test modules, 465 lines |
| Checker and policy scripts | one 41-line Node guard, a 57-line test, 2 lines in `check.mjs`, one component entry |

Tricky parts:

- The two modes (legacy and the inactive fixed-origin path) share one decision flow
  through a private `Mode` trait. The port absorbed it: one adapter per request,
  generic over the mode, so neither mode duplicates the decisions.
- Exact ordering is the interface. The tests record the port calls and audit records,
  not only outcomes: the link request that never touches, the suspended account that is
  audited with its user, trusted MFA skipping the two-factor lookup, and the email
  conflict that leaves the step open.
- The second sign-in step is someone else's slice: the port only asks whether
  two-factor is enabled and starts the existing pending step.
- `Send` futures across a generic port: the mode marker became `PhantomData<fn() -> M>`
  and `Mode::Origin` gained `Sync`.

Estimate: a row whose decisions sit in a large service file behind two modes is one
session of three code commits, about 1,500 changed lines, once its protocol primitives
have moved.
