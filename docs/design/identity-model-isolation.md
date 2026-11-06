# Identity models independent of application feature unification

## Observed coupling

At the integrated `7de528d1` checkpoint, core, storage, repositories and collaboration
requested identity with defaults disabled and no features. Isolated model-only
`cargo tree` checks passed. That did not describe the full server graph: features
on one package version unify across its consumers in the selected build.

Read-only inspection of the actual server graph found identity's `credentials`,
`password-work`, `session-tokens` and `sessions` enabled. Workspace resolver metadata
also selected its `default` feature. Normal dependency walks from core included:

```text
rbit-core -> rbit-identity -> argon2
rbit-core -> rbit-identity -> tokio
rbit-core -> rbit-identity -> tokio-util
rbit-core -> rbit-identity -> rbit-kernel
rbit-core -> rbit-identity -> zeroize
rbit-core -> rbit-identity -> sha2
```

The same identity package edge was present in repository/collaboration models and
storage's model/port adapters. This is concrete compilation/dependency coupling,
not evidence that a model read executes password hashing or starts a runtime.
Storage as a whole legitimately needs SQL, redb, encryption and Tokio; it is not
a pure model package and this extraction does not claim otherwise.

## Active extraction

`rbit-identity-models` now owns the complete prior `features=[]` model closure:

- Account entities, account names/email, their validation and shared ID aliases.
- Sign-up/login/new-user records, redacted Debug and the static account repository port.
- Session records, active-session/client projections, session-user projection and both
  static session repository ports.
- The fixed-width digest alias; hashing, randomness and encoding stay elsewhere.

It depends only on contracts and serde, exposes no application features and imports
no concrete stores or protocols. Core, storage, repositories and collaboration depend
there directly. Identity's original module/root paths reexport the exact types and
traits. Credential/session application ownership and original default crypto features
are unchanged. No registry package is introduced or upgraded.

All moved model bodies and tests are preserved. The one visibility seam is
`SessionRecord::at`: its existing deterministic constructor body is unchanged but
is public and documented so the separate session application can supply its clock.
It preserves the same Unicode client bound and field values. Session fields were
already public, so this adds no way to bypass an existing invariant or authorization.
No DTO, serde field order, integer/enum representation, validation, PHC handling,
session policy, error precedence, SQL, redb transaction or lifecycle behavior changes.

## Stronger source proof

The source check explicitly resolves the whole workspace with
`rbit-identity/password-work`, `rbit-identity/sessions` and
`rbit-collaboration/applications` enabled. It requires those features to appear in
the resolver evidence, then traverses **normal** dependency edges from core,
repositories, collaboration and identity models. It rejects identity application,
password/crypto/runtime packages and concrete stores/protocols in those closures.
Build/dev edges are deliberately outside that production-normal closure; the guard
does not claim that proc macros or the whole server build lack executable tooling.

The manifest guard also requires direct feature-free model dependencies, rejects
application edges from model consumers (including renamed/target-specific edges),
and prevents future optional application features on the model package. Normal model
dependencies are limited to contracts and serde. It checks storage's direct model
boundary, without falsely asserting storage's complete graph
is free of its own runtime/encryption dependencies.

Actual read-only resolver observations after extraction, with all required application
features enabled:

| Root | Normal package count | Internal packages in its closure |
| --- | ---: | --- |
| Identity models | 17 | contracts, identity-models |
| Repositories | 18 | contracts, identity-models, repositories |
| Collaboration | 18 | contracts, identity-models, collaboration |
| Core | 26 | access, collaboration, contracts, core, identity-models, repositories |

Counts include transitive contracts/serde dependencies. No forbidden package was
reachable. Identity itself still had credentials/default/password-work/session-tokens/
sessions, and collaboration still had applications/default, so the result cannot be
explained by accidentally disabling application functionality.

## Validation and acceptance

Source preparation passed the focused Node guard tests, complete source/dependency
policy, formatting and read-only resolver proof. Rust compilation/tests/Clippy and
structural checker execution remain pending the shared lane.

Existing account/name/record/session model unit tests move with their implementations,
including fixed JSON/postcard bytes and secret-redaction checks. New identity tests
prove legacy module/root aliases are the same concrete types and retain fixed session
wire bytes through those paths. Existing account/session application and storage
tests continue through the same reexported static port traits. Hosted CI runs isolated
models and adds the application feature union to the existing combined contract test
step rather than another broad duplicate storage/service test invocation.

```sh
node --test scripts/tests/identity-model-boundary.test.mjs scripts/tests/identity-features.test.mjs scripts/tests/collaboration-features.test.mjs
node scripts/rewrite/check.mjs
node scripts/workspace.mjs test --package rbit-identity-models --no-default-features --locked
node scripts/workspace.mjs test --package rbit-identity --no-default-features --locked
node scripts/workspace.mjs test --package rbit-core --package rbit-storage --package rbit-repositories --package rbit-collaboration --package rbit-identity --features rbit-identity/password-work,rbit-identity/sessions,rbit-collaboration/applications --locked
node scripts/workspace.mjs clippy --package rbit-identity-models --package rbit-identity --package rbit-core --package rbit-storage --package rbit-repositories --package rbit-collaboration --all-targets --features rbit-identity/password-work,rbit-identity/sessions,rbit-collaboration/applications --locked -- -D warnings
node scripts/rewrite/check-types.mjs
```

Full service/auth/web parity gates remain necessary before acceptance. One additional
crate can increase compiler-unit overhead while narrowing rebuild dependencies; there
is no measured compile-time, binary-size or runtime improvement claim. Measure matched
cold builds and warm model/credential edits, actual rustc peak memory, total compiler
wall time, artifact/cache footprint and full-server correctness before attributing a
performance benefit. This boundary is useful because it establishes actual ownership
under feature union, not because another optional flag hides the coupling.
