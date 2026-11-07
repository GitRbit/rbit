# Runner claim and lease-replay applications

This is the first active application slice of the planned `rbit-actions` feature,
not retirement of the Actions group. `RunnerClaims` owns the waiting-job claim loop
and idempotent task-token replay. It has no dependencies, runtime, queues, dynamic
registry or background work. Associated static ports retain the exact concrete
Job/Task/TaskTokenSecret/JobSpec types; they do not create a second model tree or
serialize intermediate records. Feature-owned Actions models are a later boundary.

## Production composition and preserved work

The existing scheduler delegates two private commands through borrowed `ClaimSession`.
The public poll API, HTTP protocol, runner credential cache, version-gated no-SQL idle
path, Notify registration before version read, instance poll cap, replacement-poll
cancellation and shutdown behavior remain in their current owners.

For claims:

1. Empty runner labels return before constructing the store or minting a token.
   A static closure constructs exactly one existing ActionsRunStore for a nonempty
   command, then reuses it for the whole invalid-spec retry loop. Unpolled commands
   perform no factory, token, timestamp or SQL work.
2. Generate the same task token, calculate its digest and read Timestamp::now in the
   existing ClaimRequest field order. Pass scope, labels, host labels, ephemeral flag,
   instance-runner setting and optional key directly by borrow.
3. The unchanged SQLite BEGIN IMMEDIATE claim remains sole selection/race authority:
   oldest eligible waiting job, repository switch, scope/labels, untrusted-host denial,
   environment approvals/wait timer, attempt and ephemeral gates. No preflight reads,
   changed migrations or alternative in-memory selection are added.
4. Remember a successful keyed claim before logging/assembling its specification.
   The existing job_spec retains all trust/fork/environment/secret/permission logic.
   Success remembers masks, builds the same single step-name vector, writes steps,
   reports statuses and returns the same Box<JobSpec> outcome.
5. An invalid specification fails/finalizes the task. A keyed claim returns Spent;
   an unkeyed claim loops with a new token, preserving the same store. Infrastructure
   failures propagate and do not turn into automatic retries or new claims.

For replay:

- Lookup still uses the remembered task ID, otherwise the existing atomic one-time
  per-runner restart fallback, otherwise no query. The cache logic is unchanged in
  the adapter; it does not scan all keys or add per-poll SQL.
- Foreign tasks are Unknown; non-running tasks are Spent. Only then mint a token and
  run the same replace_task_token CAS (running and token non-null). Concurrent token
  clearing returns Spent before job/spec reads.
- Load the same job, log replay, assemble specification and remember masks. Replay
  never creates steps, reports statuses, remembers a new key, or claims another job.
  Missing job/invalid specification remain Spent with their existing effects.

The generic Claimed pair moves the original two records without cloning or a heap
allocation. The existing Box<JobSpec> and step-name Vec retain their original locations; the
application adds no heap allocation beyond the original concrete command paths. No runtime latency improvement is claimed.

## Ownership and failure limits

Storage still owns its original transactions and token CAS. Claim commit, in-memory
key remembrance, specification assembly, steps/status publication and HTTP delivery
are not one durable transaction. Cancellation/assembly errors can leave a running
claim for the existing replay/sweeper paths. A returned specification is not a proof
of all descendant runner cleanup; no new lease durability, outbox or exactly-once
publication guarantee is introduced. Heartbeats, finishing, sweeper, approvals,
registration, contexts and workflow evaluation remain legacy owners for now.

## Validation and workspace integration

Root registration is deliberately pending shared-lane integration:

- Add member `crates/rbit-actions` and workspace alias
  `rbit-actions = { path = "crates/rbit-actions" }`.
- Cargo.lock needs only package rbit-actions0.1.5 (no dependencies) and the services
  package edge to it. No registry packages/features are added.
- The source policy records services-to-actions and continuously checks the new
  legacy ClaimSession component. New feature crate types use the existing strict gate.

Prepared runtime-free tests cover lazy factory/no-label/unpolled behavior, key/mask/
step/status order, keyed invalid-spec spending, single-handle unkeyed retry, normal
empty-queue token behavior, infrastructure errors, foreign/stopped/CAS-lost replay,
missing job and invalid specification. A real services fixture verifies repeat-token
rotation, old digest rejection, stable replay work and spent-key one-query behavior
while a later run waits. Existing storage Actions tests retain actual concurrent
claim, label/scope/trust/ephemeral/environment/attempt/token-CAS coverage; existing
HTTP runner tests retain protocol, idle-zero-query and lost-answer replay evidence.

Rust/AST tests are UNRUN during source preparation. After registration, use the shared
helper for rbit-actions tests and strict all-target Clippy; services integration
`actions_claims`, existing actions_runners/actions_runs and sweeper tests; storage
integration `actions`; web integration `actions_runner_tasks`/permissions/environments;
then the strict AST/graph gates and full workspace. Locked metadata cannot run in this
unregistered source checkout and is not reported as passing.

Generic claim/replay diagnostics reveal only bounded outcome names, never supplied
job/task/specification contents, and require no payload `Debug` implementation.
The borrowed session diagnostic prints only runner identity, not service/config/store
internals. Existing concrete workflow secrets already redact themselves; this
boundary additionally protects other valid composition types. Dedicated fixtures
use non-Debug payloads and a deliberately panicking Debug implementation to require
that neither normal nor alternate formatting traverses them. These Rust fixtures
remain unrun until the shared-lane registration and acceptance above.
