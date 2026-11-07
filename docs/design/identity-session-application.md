# Session application and credential-finalization ownership

This slice moves production session application decisions and password-change
attribution into identity. It preserves legacy service methods and the token,
DTO, error, SQL and redb compatibility paths. It does not complete identity/access,
MFA, recovery policy, external authentication or personal-token application ports.

## Dependency and transaction contract

Identity models and repository traits are always available with only contracts
and Serde. `session-tokens` enables the existing 32-byte codec and session token.
`sessions` additionally enables Tokio and tokio-util for rare owned credential
finalization; it does not enable Argon2 or password-work. Services explicitly
request password-work + sessions; auth requests credentials + session-tokens;
storage and core request no features and disable defaults. Workspace feature
unification is not proof that these isolated normal graphs remain lightweight.
The consumer/normal-graph guard checks the explicit matrix. Existing default
identity API remains password-work; session binaries require sessions explicitly
and are skipped without it, so a default/model-only pass cannot prove this slice.

`SessionApplication` owns creation, request resolution, listing and revocation
policy. Storage implements static associated-error repository ports by forwarding
to the existing stores. Request resolution still uses one account query supplying
active-user visibility, forced-password-change and notification flags. Missing,
suspended/deleted accounts cause session deletion. Absolute maximum age is checked
before that query. Renewal uses the same strict greater-than-one-day expiry and
at-least-five-minute last-seen thresholds; zero max-age remains unlimited. The
legacy clock sampling order, canonical IP and 256-Unicode-character agent limit
are retained. Common request reads do not spawn a task or acquire effect leases.

The redb implementation remains authoritative: reads/cleanup recheck liveness,
renewal atomically rechecks existence/expiry and never upserts, timestamps only
advance, and account-index changes share the session write transaction. The
application never turns a stale snapshot into permission to resurrect a session.
Listing remains account-scoped and store-ordered. Services map backend errors
and retain audit details after actual deletion; no audit event authorizes access.

## Admitted credential finalization

Each AccountService composition owns one clone-shared enforcement ledger and
effect scope. A synchronous gate serializes close with admission. Guards and the
three password mutations reserve a noncloneable tracked lease before their first
async wait. The saved closed-admission error is surfaced at the original password
admission boundary, preserving validation, admin authorization, lookup, throttle
and existing password-admission error precedence. Closed admission maps to the
existing generic unavailable response. A failed uncommitted request drops its
lease without creating work.

A successful verification/hash moves the lease into the existing single detached
commit/finalization task. Its narrow captures are concrete stores, audit, events,
user metadata and the identity finalizer; it does not capture the supervisor,
AccountService, ProfileService or RecoveryService. Mutation commit, authoritative
session revocation, audit and event order remain unchanged. Recovery/admin resets
await all-session enforcement directly; they do not depend on broadcast delivery.
Self-service attribution retains the existing clone-shared committed-change
ledger. The request guard keeps its current session only after matching successful
change evidence and existing deferred ownership; lag cannot claim a future change.
Dropped/failed guards release ledger ownership and perform required fail-closed
all-session enforcement. Evidence subscription panic is protected by the already
constructed guard, preventing a ghost ledger owner.

A held lease bridges close to late submission, so drain cannot observe zero before
already-admitted work is adopted. The tracking wrapper retains ownership through
future destruction; Completion RAII exists before spawning, including unpolled
runtime cancellation. Worker captures retain tracker state/statistics only, avoiding
a supervisor cycle. Dropping the request waiter or JoinHandle does not abort the
worker. Production adapters never call abort on finalization handles. Explicit
runtime destruction, panic or an external abort is abnormal and is not success.

Services closes password and session-effect admission before its first shutdown
await, then drains the effect owner alongside other feature lifecycles before host
store closure. AccountService exposes its own drain for direct embedding. Standalone
RecoveryService owns/drains a fallback scope; composed recovery uses AccountService's
scope rather than creating another owner. Cancelling a drain waiter does not drop
ownership; a later wait resumes. No persistence durability, bounded shutdown time,
crash recovery or entire-feature health guarantee follows from this scope.

Reports distinguish completed tasks, returned operation errors and abnormal or
unproved outcomes. `drained` means only admission closed and no active ownership
units; it does not imply successful mutations. No-runtime submission is abnormal.
A context from another scope is rejected before attribution or revocation effects.
Ownership units include admitted guards/leases and tracked wrappers; they are not
an independent measurement of OS/runtime task counts.

## Acceptance gates

Integration passes the isolated sessions-only configuration (34 tests), including
both new binaries without Argon2. Combined identity/auth/core/storage/services tests
pass (67 identity, 43 auth, 151 core, 530 service and 250 storage), as does strict
all-target Clippy. Source/type and isolated graph checks pass. CI compiles this
session configuration independently. The per-instance enforcement owner is shared
through one Arc in AccountService, reducing clone and startup-future footprint.
Web/server consumers and a new full hosted result still require their own gates.
The session-only integration binaries exercise application policy thresholds, exact
clock samples, account projection count, lost atomic renewal, account-scoped
revocation and token-preserving projection. Deterministic scope/enforcement tests
cover close with a held lease, late adoption, future destruction, cancelled drain
waiters, no-runtime submission, unpolled runtime drop, panic versus returned error,
foreign scope rejection, guard cancellation/drop and direct reset enforcement.
The service integration fixture blocks the real Argon2 worker before SQL commit,
closes Services, proves the admitted mutation prevents drain, then releases it and
checks late commit/revocation before shutdown completes. The shared credential
helper separately exercises caller cancellation after commit. Existing session,
recovery, password-admission and HTTP tests remain required compatibility gates.

After integration, run model-only, token-only, sessions-only, credentials-only and
combined password-work+sessions tests/checks and strict all-target Clippy. Sessions-
only must run both new binaries without acquiring Argon2. Recheck isolated model
normal trees, existing session redb race tests, all services tests and web/server
shutdown consumers. Root lock integration adds only identity -> existing tokio-util;
no external package/version change is intended. Source/type ceilings and explicit
consumer features remain required. No renderer/performance improvement is claimed.
