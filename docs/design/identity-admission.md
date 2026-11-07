# Identity work admission

Status: credential extraction, configured service admission and HTTP overload adapters
are implemented; integrated validation is in progress. Paired operational measurements
and the complete identity port remain open.

## Decision

One server instance owns one injected password-work budget. All password hashing,
verification and dummy verification share its capacity. Git/archive subprocess slots,
SSH connection limits, login rate limits and password CPU/memory capacity remain
separate resources. A request timeout must not release capacity while Argon2 still runs.

`rbit-services/src/limits.rs` already admits Git/archive work immediately, with global
and per-IP limits and a request-owned `ConcurrencySlot`. Reusing that slot around an
awaited password task would release capacity early on request cancellation. It also
couples password work to Git/network policy. Keep that limiter unchanged in this slice.
Extracting its global semaphore into the kernel can follow separately when transport
process lifetime, per-IP accounting and overload mapping have equivalent tests.

The composition root constructs a narrow `PasswordWork` handle from validated startup
configuration and a `BlockingBudget`; clones share the same budget. Its purpose is
password operations only, not a generic execution framework or credential store.
The adapter now lives in `rbit-identity`, alongside private synchronous credential
primitives. `rbit-auth::Passwords` and its error/module paths re-export the same types
for compatibility; the kernel knows no account, password, HTTP or authorization types.

Keep synchronous crypto operations private to identity for execution inside admitted workers.
Keep current Argon2id algorithm/version, parameters, salts, PHC compatibility and
malformed-hash false behavior. The existing async facade can remain temporarily for
source compatibility/test fixtures, but production services must not call its unbounded
spawn path. Enforce the production call-site inventory in CI and remove the compatibility
facade in an explicitly versioned API cleanup. Never nest an async crypto call that
spawns again inside an admitted blocking closure.

## Reservation before one-use state

The kernel reservation API is implemented; production activation remains separate:

```text
BlockingBudget::try_reserve() -> Result<BlockingReservation, AdmissionError>
BlockingReservation::spawn(self, operation) -> JoinHandle<Output>
BlockingBudget::try_spawn(operation) = try_reserve()?.spawn(operation)
```

The reservation is opaque and non-cloneable. It owns one permit; dropping it before
submission releases the permit. `spawn` consumes it and moves ownership into the worker,
including time queued in Tokio. It introduces no waiter queue or second semaphore.
Acquisition before a Tokio runtime is allowed; spawning requires a runtime and unwinding
a failed spawn must return the permit.

At the pinned baseline, `Services::check_password_and_proof` took a passkey confirmation
challenge before password verification. The replacement reserves after the existing
throttle check and before `take_challenge`, token consumption or another one-use mutation.
A rejected reservation leaves the proof usable. Once an
attempt is actually admitted, preserve the current single-use challenge semantics,
including consumption on wrong password or request cancellation after the attempt starts.

Reservations may cover a bounded database read before dispatch when that is required
to order admission before one-use state. This trades some capacity occupancy for a
clear invariant; do not hold a SQL write transaction or an unrelated network operation
under a reservation. Reject overlong password input using the existing limit before
admission. Cancellation before submission drops the reservation and starts no worker.

## Pinned-main production call-site inventory

All paths below are relative to `crates/`; line numbers are intentionally omitted so
the contract survives the extraction. Re-run `rg 'Passwords::' crates` before merging.

| Current owner and function | Work and required adaptation |
| --- | --- |
| `rbit-services/src/accounts.rs`: `authenticate_resolved` | Known-login verify and unknown-login dummy verification use the same reserved worker and propagate admission failures uniformly. Preserve the `before_verify` account-throttle callback and `(account, result)` contract. |
| `accounts.rs`: `prepare_user`, `create_user`, `sign_up`, `AccountService::create_user` | Hash a validated new password through the injected handle before database mutation. Pass the same handle through internal helpers. |
| `rbit-services/src/admin.rs`: `AdminService::create_user` | `prepare_user` must receive the shared handle; this otherwise bypasses the normal account-service entrypoint. |
| `rbit-services/src/profiles.rs`: `verify_password` | Current-password checks for username/email/password changes share the budget and existing account throttle. Preserve the returned verified hash used by compare-and-set writes. |
| `profiles.rs`: `change_password` | Verify current password and hash replacement in one admitted closure; do not finish a successful verify and then race a second admission for hashing. Keep database compare-and-set and credential finalization afterward. |
| `rbit-services/src/recovery.rs`: `reset_password` | Initial-password reuse check, when needed, and replacement hash run in one admission. Do not consume the reset token or revoke sessions on Busy/Closed. |
| `recovery.rs`: `admin_set_password` | Validate target/authorization/input, then hash through the shared handle before credential finalization. |
| `rbit-services/src/two_factor.rs`: `verify_password`, `check_password_and_proof` | Inject the shared handle, reserve before taking a passkey challenge, then reuse that reservation for the password worker. TOTP/recovery-code checks retain their existing single-use behavior. |
| `rbit-server/src/serve.rs`: `create_user` through `accounts::create_user_offline` | Use one short-lived explicitly constructed offline password-work owner for the operation; do not open redb or start all services merely to hash a CLI password. |

The pinned production inventory contains nine direct `Passwords::` invocations across
accounts, profiles, recovery and two_factor; signup/admin/CLI helpers add indirect paths.
Auth unit tests and profiles/recovery integration fixtures also use the facade and need
explicit test construction where appropriate. `ExternalAuthService` creates passwordless
accounts with an empty hash; it does not hash a generated password during registration.

## Composition and configuration

`Services::new` constructs `ServiceContext`, then `AccountService::new`,
`ProfileService::new`, `TwoFactorService::new`, `AdminService::new`, and a
`RecoveryServiceBuilder`. Construct `PasswordWork` once before those consumers and inject
clones explicitly. Public high-level service method signatures remain unchanged.
The explicit `Services::with_password_work` composition seam requires its owner's
capacity to equal validated configuration; injection cannot bypass the setting.

Avoid adding the handle to broad `ServiceContext`: only these identity consumers need
it. AccountService gets an explicit constructor argument; ProfileService can share the
account-owned handle through a narrow crate-private accessor or receive a clone directly.
TwoFactorService and AdminService receive a clone explicitly. RecoveryServiceBuilder
must require/share the same handle (including its test builder path), not silently create
another server budget. A standalone offline operation may own a separate local budget;
calling the legacy offline convenience function concurrently is not the server path.
Update manually assembled context/builders in `tests/recovery.rs` without duplicating
the live instance's budget.

Implemented immutable startup setting: `[security] max_password_jobs`, with
`RBIT_SECURITY__MAX_PASSWORD_JOBS`. Use a small explicit default of two as an initial
policy; constrained-memory and CPU measurements remain a deployment acceptance gate;
require a positive bounded value, initially 1 through 64. At current hashing parameters,
two simultaneous hashes need roughly 38 MiB for Argon2 working memory plus overhead;
this is an estimate, not a measured process RSS bound. Stored PHC parameters can differ.
Do not derive an unbounded allocation limit from CPU count or reduce Argon2 strength.

Validate during startup/admin configuration save, expose the setting/help in the admin
configuration schema, example configuration and deployment docs, and mark restart required.
Preserve defaults when loading older saved configuration through serde(default). Reject
zero rather than silently turning it into one. `rate_limit=false` must never disable this
capacity bound: request throttling and actual resource admission protect different things.
No additional queue-depth or admission-timeout knob is needed for immediate admission.

## Authentication and enumeration invariants

Known and missing logins must encounter the same capacity decision, error mapping and
dummy-work contract. Never swallow Busy/Closed through the current unit-returning
`Passwords::verify_dummy`. The application dummy operation returns a fallible result;
an unknown login always remains InvalidCredentials after admitted dummy work, regardless
of the dummy verification's boolean result.

Keep existing IP/typed-login throttle checks before expensive work. Preserve the known
account callback before verification, account-alias budgets, failure charging/reset rules,
and suspension/2FA decisions after a correct password. Within authenticate_resolved,
capacity may be reserved after record lookup and the callback so their current order
stays intact; the known and dummy branches must then consume that same reservation.
Busy/Closed are not bad passwords: do not record SignInFailed, increment guess penalties,
reset a failure budget, issue a session, or reveal which branch was selected.

Passwordless external accounts and malformed stored hashes currently fail verification
quickly. An admitted password attempt for either should perform dummy-strength work and
still return false; otherwise the missing-record path can take Argon2 time while an
existing passwordless record does not. Preserve malformed-hash false semantics, never
convert it into a successful password or expose a stored PHC value in errors. Legitimate
historical PHC parameters and database/cache latency can still differ; do not advertise
literal constant-time end-to-end login or rewrite stored parameters in this slice.

Profile replacement resets its password-check budget after successful verification,
including a confirmed replacement-hash error. A worker panic does not prove which
crypto stage completed, so it cannot reset authentication penalties speculatively.
Login/profile input-length guards are preserved. Main had no equivalent guard on MFA
password re-checks; this activation preserves that proof/failure behavior. A consistent
early input-bound policy remains an identity hardening obligation.

Do not route personal access tokens, task tokens, SSH keys, OIDC or passkey-only sign-in
through Argon2 admission. A saturated password pool must leave these independent paths
available, subject to their existing policies and resource limits.

## Error and protocol contract

Add one explicit application availability variant, for example
`ServiceError::Unavailable { retry_after_seconds }`, and map Busy/Closed to it using
the same generic user message and a short fixed Retry-After (initially one second).
Record the bounded internal reason `busy` or `closed` separately for diagnostics.
Keep crypto/worker failures as Internal; preserve InvalidCredentials, Forbidden,
RateLimited, conflicts and validation outcomes. Never disguise saturation as a failed
password, a 429 client abuse penalty, or an unexpected 500.

| Boundary | Required handling |
| --- | --- |
| `rbit-web/src/error.rs` | Add an explicit 503 mapping before the generic BadRequest fallback; retain ErrorInfo rendering, Retry-After and no-store for the unavailable response. |
| `rbit-web/src/routes/auth.rs` | Normal wrong-password login stays 422 and account suspension stays its current response. Busy/Closed flow through 503 without a session, second-factor cookie or password echo. Existing 429 form behavior remains. |
| `rbit-web/src/routes/api/error.rs` | Its exhaustive service match must emit 503 JSON with the existing error envelope and Retry-After, with no Bearer challenge. |
| `rbit-web/src/routes/git_http.rs`: `authenticate`, `Denied` | Add a distinct unavailable denial, mapped to 503 plus Retry-After. Do not fall into the current catch-all Internal mapping or produce 401/WWW-Authenticate, which can trigger credential prompts. Preserve special 2FA token-required behavior. |
| `rbit-web/src/routes/lfs.rs`: both error conversions | Propagate the Git unavailable denial and direct service availability errors as 503 in the existing LFS JSON content type, retaining Retry-After. A plain Status variant currently cannot carry the retry header. |
| HTTP token/task clients and `rbit-server/src/client` | Tokens remain fast credential checks and outside the password budget. Remote 503 remains a reported temporary failure; do not automatically replay state-changing CLI commands. |
| `rbit-ssh/src/session.rs` | SSH supports key authentication, not account-password authentication. Preserve its existing key/deploy-key and git-lfs-authenticate behavior; do not invent an SSH password path or spend Argon2 permits on key checks. |
| Offline `rbit user create` | CLI validation/errors remain visible and exit nonzero on a real failure. Its dedicated local operation normally cannot saturate; handle availability explicitly if a shared owner is supplied by an embedder. |

Audit all ServiceError matches before adding the variant: some matches are exhaustive,
others turn unknown variants into 400, 500, or generic form errors. Error middleware must
preserve the 503 status, retry header and privacy policy. Keep production messages generic
and independent of login existence, stored hash contents, or user-supplied labels.

## Credential atomicity and shutdown

All hashing precedes credential mutation. Keep `credentials::finish_update`, conditional
password-hash writes and synchronous session revocation: a disconnect after a credential
commit must not cancel revocation. A budget permit belongs to CPU work only, not to a
database mutation or the revocation task. Never use the lossy event bus to enforce it.
Reset-token consumption, password-change-required state and proof accounting must retain
their existing transaction/compare-and-set guarantees.

`Services::shutdown` currently joins background shutdowns after HTTP graceful drain;
`serve.rs` then closes the database, and `main.rs` drops the Tokio runtime before restart.
Close password admission at the start of service shutdown so delayed/new callers get
the availability result. Admitted workers are pure crypto, own their captured inputs and
may finish after their requesting future disappeared; they do not access closed stores.
Report outstanding reservations/work at shutdown. Tokio runtime teardown normally waits
for blocking workers; this slice must not claim a hard shutdown deadline or pretend a
JoinHandle abort stops Argon2. A future explicit idle/drain notification may be added to
the small kernel primitive with cancellation tests; do not add a generic supervisor now.

## Atomic delivery sequence and acceptance

1. **Kernel reservation:** add the consuming opaque reservation and tests for saturation,
   drop before spawn, panic cleanup, cancellation ownership and use outside a runtime.
   Keep existing try_spawn/error APIs as compatible sugar; no service activation.
2. **Crypto extraction:** expose the smallest synchronous hash/verify/dummy primitives
   with unchanged Argon2/Serde/PHC behavior. Add deterministic known-hash/round-trip and
   malformed/passwordless rejection coverage. No new default global concurrency state.
3. **Policy and protocol adapters:** add configuration validation/default/help and explicit
   availability mappings for HTML/API/Git/LFS, with tests for 503/Retry-After/no-store and
   unchanged 401/403/422/429 behavior. This does not activate password rejection yet.
4. **One complete production activation:** inject the shared owner into every constructor,
   replace all nine direct call sites and helper bypasses, fix dummy error propagation,
   reserve before one-use challenges, and combine multi-step crypto operations. Commit
   this with its end-to-end acceptance tests so no partially bounded live path ships.
5. **Operational verification:** run the required platform tests and paired login/load
   measurements, validate memory/latency at configured limits and publish operator guidance.
   Reconcile the feature ledger; do not claim the whole identity feature is ported.

Required focused cases:

- Hold all slots with synchronized workers. Unknown, known-wrong and known-correct login
  attempts receive identical 503 shape/retry behavior; no session/cookie/credential-failure
  audit or guess penalty occurs. Release slots and confirm original outcomes return.
- Mix login, signup, admin create, username/email/password changes, reset/admin reset and
  MFA settings concurrently: one configured aggregate ceiling, no per-service budgets.
- Drop/timeout the requesting task while crypto runs: capacity remains held to completion.
  Cancel before spawn: reservation returns and a deferred proof-consumption callback is
  untouched. Saturate before a passkey settings confirmation: retry the same proof after
  capacity returns and verify it is still usable; an actually attempted wrong proof stays
  single-use.
- Ensure current-password verify plus replacement hash cannot fail on a second admission.
  Busy/Closed must not spend reset tokens, replace hashes, alter MFA, or revoke sessions.
  Preserve the existing disconnected-request credential-finalization regression test.
- Exercise username and email aliases, suspended/2FA users, passwordless accounts, malformed
  PHC data and non-ASCII/overlong password input without reducing current strength/bounds.
- Saturate passwords while testing PAT/task-token Git and LFS, SSH keys, OIDC and passkeys:
  these remain usable. Bad token prefixes retain current refusal paths and never fall
  back to password hashing.
- Verify startup/admin saved-config defaults and invalid/zero limits, disabled rate limits,
  offline CLI creation, shutdown closure and reopening only through a new instance owner.
- Source audit finds no production unbounded Passwords facade call or nested spawn_blocking
  in the password adapter; private inputs never enter logs/metric labels.

All new modules/files/logical types stay within 400 authored lines, with constructors and
public commands documenting ownership, authorization, errors and side effects. Reusing
existing large service files is a transition seam; extract touched identity responsibilities
into descriptive modules rather than adding unrelated framework scaffolding.
