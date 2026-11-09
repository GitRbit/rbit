# Actions scheduling: run admission, concurrency and leases

Evidence for a slice of the `actions.scheduling` ledger row (dependencies, concurrency,
queue, leases, retries, cancel/rerun). The owner is `rbit-actions`. The row stays
`baseline-retained` with a progress note: this slice moves run admission, concurrency
groups and task lease checks; other parts of the row remain (see below). It follows the
[identity retirement](identity-retirement.md) pattern.

The atomic SQL in `rbit-storage/src/sqlite/actions_runs.rs` stays the race authority, as
before: creating a run with its jobs, settling jobs, claiming, cancelling, releasing,
approving, rerunning and moving leases. The moved decisions run in the same places, under
the same `Inner::groups` lock, between the same store calls in the same order.

## What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| The event allowance: runs taken one at a time, jobs per planned run, refused runs taking nothing | `runs::Allowance`, `start_run` | `rbit_actions::scheduling::{Allowance, check_jobs}` (re-exported as `runs::Allowance`) |
| `runs-on` labels are 1 to 255 characters; the workflow-error message | `start_run`, `MAX_JOB_LABEL_CHARS` | `scheduling::{check_labels, MAX_JOB_LABEL_CHARS}` |
| A repository's waiting-job cap and its message | `start_run` | `scheduling::check_waiting` |
| Entering a concurrency group: awaiting-approval runs are skipped unless `cancel-in-progress`; with it every other run is cancelled; without it held runs are cancelled and the run is held while another is active; a run never holds itself | `runs::enter_group` | `scheduling::enter_group` over `Member` (`AwaitingApproval`, `Held`, `Active`) |
| Releasing the oldest held run once no run of the group is active | `runs::release_next` | `scheduling::next_to_release` |
| A task lease: the task exists, belongs to the runner, has not ended, and is its job's current attempt; the five protocol messages | `artifacts::{running_task, leased_task}` (also used by `cache_task`) | `scheduling::leases::{running, leased, Refusal}` over the `TaskLeases` port |

Services adapters:

- `runs.rs` turns stored runs into `(RunId, Member)` in the store's order and maps `Start`
  back to `RunStart`. It then cancels the listed runs one by one, as before, following up
  each.
- `task_leases.rs` implements `TaskLeases` over the task store and answers a `Refusal` as
  before: not found, forbidden or conflict, with the same message.

## What stays (progress)

- Already prepared before this slice: runner claims and idempotent task-token replay
  (`rbit_actions::claims`, selected by the poll through `scheduler/claim_bridge.rs`).
- Still in services or storage:
  - job dependencies and `if:` decisions after each settle (`conditions.rs`, "join");
  - the poll loop and its version and `Notify` wake-ups;
  - the sweeper's zombie and timeout passes;
  - the lease checks of `task_state` and log appends, which answer differently: a
    stopped task is told to cancel, and a closed log has its own message;
  - cancel, rerun and approval request validation;
  - the review gate and wait timer in SQL.

  Each is either SQL race authority or I/O orchestration, and is left for later slices.

## Boundary that fails if the moved decisions return

`scripts/rewrite/actions-scheduling-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
rejects in the services, web and server production sources:

- the admission messages or a `MAX_JOB_LABEL_CHARS` constant;
- allowance arithmetic;
- the group loop (`let mut busy`, or the held-or-awaiting release test);
- the rerun lease message;
- `running_task` or `leased_task` outside `task_leases.rs`.

Status sets that mean something else (hooks, closing pull requests) are allowed. Against
the legacy files at `eb21e83a` it reports 5 violations; against this tree, none.
`scripts/tests/actions-scheduling-retirement.test.mjs` pins the rules and the current tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same order of store calls, same cancellations in the same order, same `RunStart`, same workflow-error texts and protocol answers. The run, race and claim suites (`actions_runs`, `actions_claims`, the storage `actions` tests, the artifact and cache suites) are unchanged. |
| Security | Untrusted runs still neither enter nor hold groups while awaiting approval; a task left behind by a rerun still stores nothing; another runner's task is still forbidden. |
| Migration and rollback | No schema, migration or route change. Rollback is the previous binary. |
| Performance | Two small vectors per group decision; same queries. No release measurement claimed. |

## Results

A quick run of the repository's own CI jobs at the third commit (`16a94492`, base
`eb21e83a`) passed rewrite contracts (with the logical-type check), workspace-wide
formatting and all-target Clippy with `-D warnings` (which compiles every crate), licenses
and advisories, and the front-end build. Nextest ran the changed packages, `rbit-actions`
and `rbit-services`: 822 tests, 822 passed, none skipped. That set includes these,
unchanged and passing:

- `actions_runs`: concurrency, holding, release, cancel, rerun and approval;
- `actions_claims`, `actions_pulls` and `actions_dispatch`;
- the artifact and cache upload and lease suites;
- `hostile_workflows`.

The new `rbit-actions` `scheduling::tests` passed too. Storage's own `actions` race tests
(the SQL is unchanged) and the `rbit-web` suites were not part of the quick run. Windows
legs were not run. Locally, a `check` of `rbit-actions` and `rbit-services` (all targets)
was clean.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record) |
| `rbit-actions` production | 3 files, +206 (`scheduling`, `scheduling/leases`, wiring) |
| `rbit-services` production | 4 files, +107 / −75 (one new 66-line adapter) |
| Tests | 1 new file, +115 |
| Checker and policy scripts | one 35-line Node guard with a 39-line test |

Tricky parts:

- `enter_group` cancelled runs inside its loop. The decision is now a list and the
  cancellations follow it, which keeps the same calls in the same order: each
  cancellation's own result never changed whether the group was busy.
- The lease checks lived in `artifacts.rs`, owned by the artifacts slice, and were reached
  through `cache_task` too. Moving them behind one port changed no caller.

Estimate: the remaining parts are each about one session, except the sweeper and the
`if:` decisions after settles. Those need the storage transitions modelled as ports first.
