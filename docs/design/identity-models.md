# Identity model and credential feature boundary

Identity owns `AccountName`, `Email`, `AccountKind`, `Account` and `User`. Their
public constructors, fields, methods and existing Serde representations stay the
same. Legacy `rbit-core` root and account/names module paths reexport those exact
types, including tuple-constructed shared AccountId and Timestamp fields. No data
conversion, account lookup or new persisted representation accompanies this move.
RepositoryName stays core-owned and uses the shared contract grammar independently.

The existing permissive email plausibility checks are preserved, including trimming,
case, Unicode and other accepted edge cases. Account names keep their original route
reservation list, byte bounds and allocation-preserving owned constructor. This
extraction does not tighten validation or claim to verify mailbox ownership.

## Explicit feature graph

The existing identity crate provides three small layers:

| Selection | Available API | Normal dependencies |
| --- | --- | --- |
| No default features | Account domain models and validated names | contracts, Serde |
| `credentials` | Models plus existing Passwords/PasswordError API | above plus Argon2, thiserror, Tokio |
| `password-work` | Above plus injected PasswordWork/PasswordAttempt | above plus kernel, zeroize |
| Default | Complete existing password API | same as password-work |

The asynchronous legacy Passwords facade inherently needs Tokio. Model-only
consumers do not activate it. The workspace dependency must disable defaults;
core uses that model-only dependency, auth explicitly enables credentials, and
services plus web development fixtures explicitly enable password-work. Cargo
unifies requested capabilities when building the server; feature selection is
compile-time dependency organization, not a runtime security sandbox.

No new crate, global budget, crypto algorithm, configuration value or execution
owner is introduced. There is one shared password owner per composed service
instance as before. Workflow has no core/identity dependency, and runner depends
on workflow plus its own existing runtime; neither receives a new identity edge.

The five existing password integration binaries require password-work explicitly.
Cargo skips them when that feature is absent. A successful model-only run therefore
proves only model behavior and dependency isolation, never crypto/cancellation
acceptance. Credentials-only builds must still exercise their applicable unit tests;
default/password-work validation must run all existing password integration cases.

## Integration and validation

Root composition must change its workspace declaration to
`rbit-identity = { path = "crates/rbit-identity", default-features = false }`.
The leaf manifests request their explicit capabilities. Architecture policy permits
core -> identity for compatibility; identity itself permits only contracts/kernel
feature edges and never imports core, auth, storage, HTTP or services. Root updates
the lockfile to add identity's contracts/Serde and model-test dependency edges.

Use separate helper runs after the shared build lane is available:

```sh
node scripts/workspace.mjs test --package rbit-identity --no-default-features --locked
node scripts/workspace.mjs test --package rbit-identity --no-default-features --features credentials --locked
node scripts/workspace.mjs test --package rbit-identity --locked
node scripts/workspace.mjs test --package rbit-core --package rbit-auth --locked
```

Inspect `cargo tree --edges normal` separately for identity without defaults and
core, then credentials/password-work. The model closure must contain no Argon2,
Tokio, zeroize or kernel edge introduced by identity. Workspace-wide builds unify
features and cannot establish this isolated graph. Verify normal/all-target checks,
strict Clippy, source/type ceilings and existing service/HTTP tests afterward.
The integrated feature matrix passes six model-only tests, eight credentials-only
tests and all thirty default tests, including every existing password integration
binary. Core/auth compatibility passes another 151/42 tests. Strict all-target Clippy
passes for model-only, credentials-only and the combined default consumer graph.
Isolated normal core metadata contains 24 packages and no Argon2, Tokio, kernel or
zeroize edge. CI checks both explicit consumer features and resolved model-only
normal graphs; workspace feature unification is not substituted for that evidence.

Account registration/authentication/session application workflows and persistence
ports remain the next production port. Core no longer owns these model definitions,
but that fact alone does not complete account/session application ownership, MFA,
recovery, external sign-in, tokens, or phase2.
