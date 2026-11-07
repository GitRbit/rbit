# Account registration and password authentication application port

Identity now owns registration validation, bootstrap/setup checks, live closed/open
policy combination, provisioning mode selection and password-authentication decision
order. Legacy service entry points delegate to those applications; the feature imports
no core, auth, services, concrete store, HTTP type, audit service or event bus.

The always-available AccountRepository trait and SignUp/NewUser/LoginRecord records
use model types and standard futures only. Storage implements the trait using its
existing AccountStore transactions. Its identity dependency disables defaults and
requests no credential feature. The models-only normal dependency closure stays
independent of Argon2, Tokio, kernel and zeroize introduced by identity; the guard now
covers storage's capability declaration too. Applications are gated by password-work.
Constant-time setup comparison and bootstrap randomness add optional existing
workspace subtle/getrandom dependencies to that layer only.

## Required business order

Signup counts users, then reads live registration policy, rejects closed non-bootstrap
registration, checks the exact trimmed setup token for the first user, validates names/
email/password, reserves the shared password owner, hashes and calls the repository.
The preflight count never decides the first administrator. SQL register_user still
checks first-user/closed-registration state in its original write transaction and
ignores the submitted admin flag. Explicit and initial-password provisioning delegate
to their existing separate SQL transactions. No schema, migration, table, uniqueness
constraint or first-administrator race policy changes here.

Authentication protects plaintext, preserves the original 1024-byte early bound, looks
up a trimmed login, calls before_verify with the resolved account or None, then asks
for shared password admission. A callback error retains precedence over Busy/Closed.
Covering work for missing/malformed PHC stays in PasswordAttempt; suspension is exposed
only after a valid password. Infra/crypto failures retain their typed errors rather
than becoming bad-password results. The ordinary pre-admission callback is read-only
throttle checking, not failed-login accounting.

Applications receive references to the SAME injected PasswordWork owner. No operation
creates its own budget; offline CLI setup explicitly constructs its one process-local
owner through the existing validated configuration adapter. Admission errors map to
ServiceError at the existing metrics/protocol boundary exactly once. Busy/Closed do
not add failed-login surcharges, audits, sessions or account mutations.

## Adapters and side effects

Services supplies live instance settings and error translation. It retains success
logs, UserSignedUp publication AFTER commit, and administrator audit publication in
the same order. Administrator authorization precedes validation, hashing and store
writes. SQL-only offline creation emits no event as before. Existing account/session
public API call sites continue to await the legacy methods. Storage's public NewUser
and LoginRecord names and the services SignUp name reexport the exact feature types.

SignUp/password authentication entry functions synchronously capture plaintext in
Zeroizing ownership before returning futures, including unpolled and early-error
futures. The legacy account and admin/offline adapters protect their entry ownership
too. Auth PHC input is protected before callback/admission early exits. Credential
fields in SignUp/NewUser/LoginRecord Debug output and the registration setup token
are redacted. Raw SignUp still has String fields for API compatibility: caller-held
values before entry, and unrelated outer request/service futures, are not claimed to
have been globally migrated to a zeroizing contract. Persisted hashes/SQL row buffers
retain their existing lifecycle; this is not a claim of universal secret erasure.

## Acceptance and remaining scope

New port tests cover setup and live-policy precedence, SQL-authority rejection,
provisioning modes, validation versus admission, callback/store-error precedence,
Busy/Closed shapes, suspension and missing/corrupt credentials, dropped unpolled work,
redacted Debug output and protected payload ownership. Real-service regressions cover
unauthorized provisioning under closed work, initial-password flags, post-commit event
count, SQL conflict suppression and exact type compatibility. Existing bootstrap races,
audit/throttle, session and HTTP acceptance tests remain authoritative.

Integration adds the reviewed storage -> identity seam and optional-dependency lock
edges without changing external versions. Models-only (seven tests), credentials-only
(nine tests) and the combined identity/core/auth/storage/services test command pass.
The combined run includes 535 service tests and 249 storage tests, retaining bootstrap,
audit/throttle, session and migration regressions. Strict all-target Clippy passes
after introducing the descriptive AuthenticationOutcome alias for its result tuple;
all eight application-port tests also pass after test-fixture lint corrections.
Reproduce the main application checks with:

```sh
node scripts/workspace.mjs test --package rbit-identity --locked
node scripts/workspace.mjs test --package rbit-storage --package rbit-services --locked
```

Repeat the isolated model/credentials/password-work feature matrix and normal graph
checks, strict Clippy, source/type policy and existing HTTP acceptance afterward.
Session resolution/renewal/listing/revocation and password-change enforcement still
live in services/storage and remain the next application port. Recovery, MFA, external
sign-in and token feature groups remain pending; this subset does not complete phase2.
