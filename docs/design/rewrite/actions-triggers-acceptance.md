# Actions trigger retirement acceptance

Acceptance evidence for the `actions.triggers` ledger row ("Push/pull/dispatch/schedule/
workflow events and immutable trust inputs"), following the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Pushed refs considered: live branches and tags, push order, at most 16 | `rbit_actions::triggers::considered_refs` |
| Default-branch head that refreshes the schedule | `triggers::default_branch_head` |
| `paths` comparison base (previous tip; a new branch against the default branch) | `triggers::comparison_base` |
| Changed-path cap (3,000) and unknown paths without a merge base | `triggers::{LISTED_CHANGED_PATHS, changed_paths_known}` |
| Where a push's skip marker is looked for (full head message) | `triggers::head_message` |
| Pull request events to activities; `synchronize` only from pushes that may start runs; cause (pusher, else author) | `triggers::{pull_trigger, pull_cause}` |
| Closing a pull request cancels waiting, held and awaiting-approval runs | `triggers::cancelled_when_pull_closes` |
| Evaluation budget, payload commit count | `triggers::{EVALUATION_BUDGET, PAYLOAD_COMMITS}` |
| `workflow_run` chain depth (3) | `triggers::events::{MAX_WORKFLOW_RUN_CHAIN, chain_continues}` |
| `repository_dispatch` event type and `client_payload` limits and messages | `events::{dispatch_event_type, client_payload_problem, CLIENT_PAYLOAD_NOT_OBJECT}` |
| `workflow_dispatch` refs: own branches, then tags; never `refs/pull/*` or commit ids | `events::{dispatch_ref, RefSide, REF_REFUSED}` |
| Schedule tick, pause after failures, due per tick, unreadable retry; an entry waits while its previous run is waiting or held | `events::{TICK, PAUSE_AFTER_FAILURES, DUE_PER_TICK, RETRY_UNREADABLE, schedule_waits_for}` |
| Projections of `RefUpdate`, `IssueAction`, run `Status` and the workflow activity type | `rbit-services` `actions/trigger_inputs.rs` (types only) |
| Workflow discovery, `on:` matching, `types`/`branches`/`paths` filters, skip-marker syntax, inputs | `rbit-workflow` (row `actions.discovery-language`, ported) |
| Pull request trust classification and the fork override | `rbit-services` `actions/trust.rs` (row `actions.fork-policy`) |

The services adapters (`triggers.rs`, `chains.rs`, `dispatch.rs`, `schedules.rs`)
keep the domain-event subscriber, settings and repository lookups, Git reads, the
bounded evaluation semaphore, payload building and run creation. `rbit-actions` gains
no dependency: every decision takes borrowed projections, so its `Cargo.toml` is
untouched and `lib.rs` gains one line.

Trust inputs: push, schedule, `workflow_dispatch`, `repository_dispatch` and
`workflow_run` runs are created trusted, pull request runs with the classification of
`actions.fork-policy`; `Trigger::trust` is fixed when the run is created and stored as
`Run::trusted`. That composition is unchanged and stays with the run creation in
`runs.rs` (row `actions.scheduling`).

## Legacy code removed

- In `triggers.rs`: the four limit constants, the ref filter and cap, the default-branch
  head search, the activity match for pull request actions with its `synchronize`
  guard, the close-cancellation status set, the head-message builder, the new-branch
  base match and the changed-path cap and merge-base check.
- In `chains.rs`: the chain and `repository_dispatch` limit constants and their checks.
- In `dispatch.rs`: the ref-prefix parsing and refusal message.
- In `schedules.rs`: the tick, pause, due and retry constants and the waiting-status
  check.

Public constant paths stay as re-exports (`rbit-web` reads
`rbit_services::actions::chains::MAX_DISPATCH_EVENT_TYPE_CHARS`). `triggers.rs` went from
505 to 470 lines.

## Boundaries that fail if legacy paths return

`scripts/rewrite/actions-triggers-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
over the services, web and server sources, comments ignored, rejects:

- redefined trigger, dispatch, chain or schedule limits;
- the ref cap, changed-path cap or overflow check applied in an adapter;
- the new-branch comparison base;
- `synchronize` gating on `starts_runs()` inside an action match;
- the close-cancellation and schedule-waiting status sets;
- chain-depth and dispatch-limit comparisons;
- the dispatch ref refusal text or prefix parsing.

`scripts/tests/actions-triggers-retirement.test.mjs` covers each rule, the allowed
projection/re-export/web-validation cases and the whole tree.

## Results

Linux quick runs (format, workspace clippy, contracts, deny, assets, and nextest for the
changed packages) on both code commits, plus local runs:

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check`, workspace clippy `-D warnings` | pass |
| Tests for the changed packages (797 tests, including `rbit-actions` and the `rbit-services` Actions suites `actions_pushes`, `actions_runs` and the dispatch, chain and schedule tests) | pass |
| Rewrite contracts, including `check-types.mjs` for the strict `rbit-actions` crate | pass |
| Front-end build, licenses and advisories | pass |
| `node scripts/rewrite/check.mjs` | 0 violations |
| `node --test scripts/tests/actions-triggers-retirement.test.mjs` | pass |

New unit tests in `rbit-actions` (`triggers::tests`, `triggers::events::tests`) cover ref
selection and the cap, default-branch head and comparison bases, the changed-path rule,
head messages, pull request activities and causes, close cancellation, chain depth,
`repository_dispatch` limits and messages, dispatch ref resolution and the schedule
waiting rule. Existing suites ran unchanged. The full workspace suite and the Windows
legs are left to the batch run.

## Parity argument

- Ref selection keeps the same predicate (`after` present; `refs/heads/` or `refs/tags/`),
  order and cap; `RefUpdate::{is_deleted, branch, tag}` are those prefix and `after` tests.
- The comparison base, cap (`max_files` 3,001, refuse above 3,000) and merge-base rule are
  the same expressions; skip markers are matched by the same `has_skip_marker` on the
  same text.
- Pull request activities map one to one; a `synchronize` from a push that may not start
  runs returns no runs before any storage read, as before. The cause is the same
  `unwrap_or`.
- The close-cancellation set and the schedule-waiting set are unchanged
  (`Blocked` is projected as held).
- `repository_dispatch` checks run in the same order with the same messages, and the
  payload length is computed only when the property count passes. A dispatch ref is
  refused before any ref is read, as before; branches are tried before tags.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; 0073 remains the latest migration.
Rollback is the previous binary. Every decision is a constant-time borrowed check on
data the adapters already held; no query, Git read or allocation was added beyond the
projections' borrowed views.

## Not covered

- Pull request trust classification (`actions.fork-policy`) and run creation, approval
  and concurrency (`actions.scheduling`) are separate rows.
- `max_schedules_per_repository` is configuration applied as a cap in `refresh_schedules`.
- The full workspace suite and Windows legs are left to the batch run.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (plus the guard and this record) |
| Files changed by code commits | 10 |
| Lines added / removed (all commits before this record) | 508 / 103 |
| Production Rust (`src`, non-test) | `rbit-actions` +201 (decisions 139 + 121 less tests), services −53 net across four adapters plus a 47-line projection module |
| Tests | 59 + 26 lines of new `rbit-actions` unit tests |
| Checker and policy scripts | a 42-line guard, a 48-line test, two checker lines |

Tricky parts:

- `rbit-actions` has no dependencies and other agents edit its manifest, so the
  decisions take borrowed projections (`PushedRef`, `PullEvent<P>`, `RunStage`, `RefSide`)
  rather than `rbit-core`, `rbit-git` or `rbit-workflow` types; the services projection
  module translates types only.
- `triggers.rs` sat at its 505-line ceiling: the projections went to their own module
  so the adapter shrank instead of growing.
- Keeping refusal order: the `synchronize` gate, the dispatch ref refusal and the payload
  length check still run before any storage or Git read, as before.

Estimate for similar rows: an adapter-heavy Actions row whose decisions are pure checks
is one session of about two code commits, 10–15 files and 400–600 changed lines. Rows
whose decisions sit inside SQL transactions (`actions.scheduling`) are several times
larger.
