# Actions secrets and environments: first slice

Evidence for the first slice of the `actions.secrets` ledger row (sealed scoped secrets,
variables, environment approvals and timers). The owner is `rbit-actions`. The row stays
`baseline-retained`: this slice moves the rules, while the sealing mechanism and the SQL
gates stay in their adapters for the reason given below. It follows the
[identity retirement](identity-retirement.md) pattern.

## What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| Value limits and checks (1 to N bytes, no NUL, the messages), `MAX_VARIABLE_BYTES`, `MAX_PER_SCOPE`, the `UNREADABLE` message | `rbit-services/src/actions/secrets.rs` | `rbit_actions::secrets::{check_value, MAX_VARIABLE_BYTES, MAX_PER_SCOPE, UNREADABLE}` (re-exported at the services paths) |
| A repository or environment secret is visible to its repository only | `ActionsService::put_secret` | `secrets::check_visibility` |
| A task opens its secrets in store order, the first of each name winning, never opens a shadowed one, and fails closed naming the first it cannot open | `ActionsService::task_secrets` | `secrets::open_task_secrets` |
| An environment's secrets fail closed the same way, in order, without deduplication | `ActionsService::environment_values` | `secrets::open_environment_secrets` |
| Branch policy type | `rbit_core::actions::BranchPolicy` | `rbit_actions::environments::BranchPolicy` (re-exported at the old path) |
| Ref gate: branch or tag from the full ref, any / protected branches / selected patterns, protection read only for a protected policy and a branch, the refusal message | `ActionsService::environment_values` | `environments::DeploymentGate` over the `RefRules` port |
| Selected patterns as full-ref patterns (`main` names a branch, `refs/tags/v*` a tag, `!` kept) | `environments::ref_patterns` in services | `environments::full_ref_patterns` |
| Review rights (listed and able to read; self-review refusal and message) | `environments::may_review` in services | `environments::may_review` |
| Reviewer and wait timer bounds and messages | services and `MAX_REVIEWERS` in the environment store | `environments::{check_reviewer_count, check_wait_timer, MAX_REVIEWERS, MAX_WAIT_TIMER_MINUTES}`; the store re-exports the reviewer bound through `rbit_core::actions::MAX_ENVIRONMENT_REVIEWERS` |

Services keep calling the same `ActionsService` methods with the same messages, error
kinds and query order. `StoredRefRules` (services) answers the gate's port from
`branch_protection().list` and the workflow glob engine.

## What stays, and why

`rbit-core` depends on `rbit-actions` (the reviewed fork-policy edge), and the unified
model boundary (`scripts/rewrite/identity-model-boundary.mjs`) forbids `zeroize`,
`base64`, `sha2`, `getrandom` and `subtle` in `rbit-core`'s resolved graph. Cargo
features cannot hide them: the check resolves the workspace with feature unification.
So `rbit-actions` must stay free of cryptography and of `rbit-workflow` (which depends on
`zeroize`). That keeps four things where they are:

- the key file, AES-256-GCM sealing and opening, and the associated-data format
  (`rbit-services/src/actions/secrets.rs`, `SecretsKey`, `load_key`);
- the glob engine that matches full-ref patterns, and the name and pattern validation
  that needs it (`put_environment`'s first checks);
- the review gate and the wait timer, which are atomic SQL in `actions_runs` (settle and
  claim);
- environment, secret and variable storage (`actions_environments.rs`, `actions_secrets.rs`).

Moving the first two needs either the fork-policy edge replaced (for example by moving
`token_permissions`'s trust argument off `RunTrust`) or a crypto-free crate for the
shared types; that is an architecture decision for review, not part of this slice. Moving
the SQL gates belongs to `actions.scheduling`.

## Boundary that fails if the moved rules return

`scripts/rewrite/actions-secrets-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
rejects in production sources outside `rbit-actions`:

- a `BranchPolicy` type;
- a redefined `MAX_PER_SCOPE`, `MAX_VARIABLE_BYTES`, `MAX_WAIT_TIMER_MINUTES`, `MAX_REVIEWERS` or `UNREADABLE`;
- the environment protection messages (deploy refusal, self-review, reviewer and timer bounds);
- full-ref pattern rewriting;
- in the services and server adapters, the secret value and scope messages.

The REST API's own refusal of a `visibility` field on repository secrets is a request-shape
rule and is allowed. Against the legacy files at the branch's start it reports 7 violations;
against this tree, none. `scripts/tests/actions-secrets-retirement.test.mjs` pins both.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same messages, order and laziness; opening order, deduplication and fail-closed unchanged; sealing code untouched. The unchanged `selected` pattern tests in services now exercise `full_ref_patterns` plus the glob engine, and the unchanged secrets unit tests exercise the delegated `check_value`. Existing suites listed in Results pass unchanged. |
| Security | "Approval never grants secrets" is still `trust::Grants`; untrusted runs, environment protection and fail-closed opening are unchanged; values never reach `Debug` or logs. |
| Migration and rollback | No schema, migration, stored format or route change. Rollback is the previous binary. |
| Performance | Same queries; one name vector per task or environment with secrets. No release measurement claimed. |

## Results

A quick run of the repository's own CI jobs at the third commit (`8a3577c6`, base
`42aca5a2`): rewrite contracts (including the logical-type check), workspace-wide
formatting and all-target Clippy with `-D warnings`, licenses and advisories and the
front-end build passed. Nextest ran the changed packages (`rbit-actions`, `rbit-core`,
`rbit-services`, `rbit-storage`): 1,405 tests, 1,405 passed, none skipped. Among them, all
passing and unchanged: `rbit-services` `actions_secrets`, `actions_settings` and the
environment and secrets unit tests; `rbit-storage` `actions` (secrets, environments,
reviews). The new `rbit-actions` tests (`secrets::tests`, `environments::tests`) passed.
`rbit-web` suites (`actions_environments`, `actions_settings`, `actions_fork_runs`) were not
part of the quick run; the integration batch's full check covers them. Windows legs were not
run. Locally, a `check` of the touched crates was clean.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| Files changed by code commits | 15 |
| `rbit-actions` production | 5 files, +227 / −1 |
| `rbit-core`, `rbit-storage`, `rbit-services` production | 6 files, +68 / −103 |
| Tests | 2 new files, +134 |
| Checker and policy scripts | one new 38-line Node guard, a 39-line test |

Tricky parts: the first attempt moved sealing into `rbit-actions` and failed the model
boundary at once, since the fork-policy edge puts everything `rbit-actions` links into
`rbit-core`'s graph. Rules without dependencies could move; the mechanism could not.

Estimate: the remainder of this row (sealing, the key file, the glob-bound checks) is
about one session once the edge decision is made. The SQL gates go with
`actions.scheduling`.
