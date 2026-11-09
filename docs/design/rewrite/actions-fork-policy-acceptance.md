# Actions fork policy acceptance

Acceptance evidence for the `actions.fork-policy` ledger row: approval never grants fork
runs secrets, variables or write credentials. The owner is `rbit-actions`. It follows the
[identity retirement](identity-retirement.md) pattern.

## What moved

Before this slice the policy had three homes: the pure table and types in
`rbit_core::actions` (`TriggeringActor`, `AuthorStanding`, `RunTrust`,
`classify_pull_request`); the fact gathering, read order and fork override in
`rbit-services/src/actions/trust.rs`; and hand-written `run.trusted` checks wherever a
stored run receives something.

| Decision | Legacy place | Now |
| --- | --- | --- |
| Trust types and the classification table (writers trusted, suspended actors wait, a vouched author's own push skips the wait) | `rbit_core::actions` | `rbit_actions::trust::{TriggeringActor, AuthorStanding, RunTrust, classify_pull_request}` |
| Fact order and short-circuits (actor; then the author's suspension, trusted listing, approval policy, merged pull requests) and "a fork is never trusted, a writer only skips the wait" | `ActionsService::{classify_pull_request, triggering_actor, author_standing}` | `PullRequestTrust::classify` over the `TrustFacts` port |
| A stored run's trust is its `trusted` flag alone; approval changes nothing | `tokens.rs` rebuilding `RunTrust` | `RunTrust::from_stored` |
| Secrets, variables, environment values and log masks only for trusted runs | `run.trusted` checks in `scheduler.rs`, `secrets.rs` (×2), `environments.rs`, `logs/masking.rs` | `Grants::of_stored(..).secrets_and_variables` |
| Task tokens read at most for untrusted runs | `token_permissions` with a hand-built `RunTrust` | `token_permissions` with `RunTrust::from_stored` (`Grants.write_token` states the same rule) |
| Concurrency groups and cache write scopes of untrusted runs live under `pr-<n>/`, `untrusted/`, `pull/<n>` or `untrusted/<run>` | `runs::namespaced_group`, `scheduler::cache_scopes` | `Isolation` |

`rbit-services/src/actions/trust.rs` is now only the storage adapter (`StoredFacts`): it
reads accounts, suspension, repository grants, the trusted list, the effective approval
policy and merged pull requests, exactly the queries it made before, in the same order.
Every caller keeps calling `ActionsService::classify_pull_request`.

`token_permissions` itself stays in `rbit_core::actions`: it resolves the
job/workflow/default declaration over `TokenPermissions`, which storage, web and the
settings API share. Its trust clamp takes `trusted: bool`; the only production caller
passes `RunTrust::from_stored(..).is_trusted()`.

## No rbit-core → rbit-actions edge

This row first added `rbit-core → rbit-actions` so that `rbit_core::actions::RunTrust`
stayed a re-export and `token_permissions` kept a `RunTrust` argument. Once the artifact
store moved into `rbit-actions` (which then depends on `rbit-workflow`, and through it on
`sha2` and `zeroize`), that edge pulled those crates into the model graph of `rbit-core`.
The edge and the re-export were removed: the clamp argument is a `bool`, and
`crates/rbit-services/tests/actions_pulls.rs` imports `RunTrust` from
`rbit_actions::trust`. Behaviour is unchanged.

## Legacy code removed

- `rbit_core::actions::{TriggeringActor, AuthorStanding, RunTrust, classify_pull_request}`
  definitions (now only at `rbit_actions::trust`), and the table test, which moved
  unchanged to `rbit-actions` (`trust::tests::pull_request_trust_table`).
- `ActionsService::{triggering_actor, author_standing}` and the fork override in services.
- The direct `run.trusted` grant checks, the hand-built stored `RunTrust` in `tokens.rs`,
  and the namespace `match`es in `runs.rs` and `scheduler.rs`.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-fork-policy-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
scans production sources of every crate except `rbit-actions` (comments stripped) and
rejects:

- a `RunTrust`, `TriggeringActor` or `AuthorStanding` definition;
- a `classify_pull_request`, `triggering_actor` or `author_standing` function, unless it is
  the services adapter and delegates to `PullRequestTrust::classify`;
- built `TriggeringActor::Known` or `AuthorStanding::*` decision inputs;
- `pr-{…}` or `untrusted/{…}` namespaces built by hand;
- in services and server, any read of `run.trusted` that does not go through
  `RunTrust::from_stored`, `Grants::of_stored` or `Isolation::of`.

The presentation flag in `rbit-web` (`untrusted: !run.trusted`) and the cache API's own
`pull/<n>` scope parsing are allowed. Run against the legacy files at `4868231e`, it
reports 13 violations; against this tree, none.
`scripts/tests/actions-fork-policy-retirement.test.mjs` pins both.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same table, same fact queries in the same order with the same short-circuits, same error propagation, same stored flag and same namespace strings. `actions_pulls` (including the trust table against real accounts and the fork override), `actions_fork_runs` and the token, secret, environment, cache and concurrency suites pass unchanged. |
| Security | The cross-run guarantee (approval never grants secrets, variables or write tokens) is tested end to end by `crates/rbit-web/tests/actions_fork_runs/` and is unchanged. New unit tests prove grants depend only on stored trust, never on approval. |
| Migration and rollback | No schema, migration, stored format or route change; latest migration remains 0073. Rollback is the previous binary. |
| Performance | The same database reads per classification; one static, monomorphised call layer. No release measurement claimed. |

## Source under test

The code commits on top of `4868231e`:

1. Decide pull request trust and run grants in rbit-actions.
2. Classify and grant Actions runs through rbit-actions trust.
3. Fail the rewrite check if retired fork policy returns.

## Results

One Linux run of the repository's own CI jobs at the third commit (rebased onto
`ee9f958c`): formatting and all-target Clippy with `-D warnings`, licenses and
advisories, and the front-end build passed. Nextest ran 3,287 tests: 3,287 passed and 2
were skipped (the skips predate this change). None failed. The rewrite-contracts job
passed `check.mjs` (0 violations) and its script tests. Its logical-type step failed only
on `rbit-storage/src/identity_access_tokens.rs` ("cannot resolve self type
AccessTokenStore"), a known problem in the base stack that this slice does not touch.
Windows legs were not run.

Fork-policy groups within that run, all passing:

| Group | Tests |
| --- | --- |
| `rbit-actions` unit: `trust::tests` (the moved table, grants, isolation) | 3 |
| `rbit-actions` integration: `trust` (fork override, standing order and short-circuits, unknown and gone accounts, error propagation) | 4 |
| `rbit-services` `actions_pulls`, unchanged: the trust table against real accounts, forks and readers wait with a read-only run, vouched authors skip the wait but never get secrets, synchronize judged by the pusher, merged pull requests vouch | 8 |
| `rbit-web` `actions_fork_runs`, unchanged: fork runs wait unclaimed, then run without secrets, variables or write access; approval holds only for its own commit and workflow; trusting the author never lifts the restrictions; leases and uploads | all passed |
| `rbit-core` `token_permissions` table, unchanged (through the re-exported `RunTrust`) | passed |

Locally: `node scripts/rewrite/check.mjs` reports 0 violations, `cargo fmt --all --check`
passes, and the rewrite script tests (`actions-fork-policy-retirement`,
`actions-discovery-retirement`, `rewrite-*`) pass. No local compilation was done; the
shared build lease was busy.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger row) |
| Files changed by code commits | 24 |
| Lines added / removed (all) | 587 / 182 |
| `rbit-actions` production | 5 files, +242 / −1 (`trust` with its port and application) |
| `rbit-core`, `rbit-services` production | 13 files, +73 / −180 |
| Tests | 2 new files, +178 (the moved table, grants, isolation, scripted-port application) |
| Checker and policy scripts | one new 46-line Node guard, a 41-line test, one reviewed edge |

Tricky parts:

- `rbit-actions` may not depend on `rbit-core`, so the types had to move rather than be
  wrapped. Keeping the row's own acceptance test unchanged needed a re-export, hence the
  one reviewed edge.
- Read order is part of the contract (a failing or slow query must fail the same way), so
  the application owns the order and the port exposes one fact per query.
- The guarantee was enforced by six scattered boolean checks. Giving each a named grant
  made them checkable by the boundary without changing what any of them returns.

Estimate: once a row's pure decision exists somewhere, moving it behind a port with its
adapter is one session and three code commits, as here. Rows still decided inside SQL
(`actions.scheduling`, `actions.secrets`) remain larger.
