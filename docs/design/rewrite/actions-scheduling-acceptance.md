# Actions scheduling retirement acceptance

Acceptance evidence for the `actions.scheduling` ledger row (dependencies, concurrency,
queue, leases, retries, cancel/rerun). The owner is `rbit-actions`, and the row is
`ported`. Three slices retire it, each recorded below: the first moved run admission,
concurrency groups and task lease checks (`rbit_actions::scheduling`); the second the
long poll and the sweeper's stale-task passes (`rbit_actions::polling`,
`rbit_actions::stale_tasks`); the third the job decisions after each settle, the cancel,
rerun, approval and review request rules and the lease answers
(`rbit_actions::scheduling::{jobs, requests, leases}`). Runner claims and idempotent
task-token replay were prepared earlier (`rbit_actions::claims`). The services keep
adapters only: `actions/task_leases.rs`, `actions/scheduler/poll_queue.rs` and
`actions/stale_task_port.rs`. The review gate and the wait timer stay SQL predicates by
design: they are evaluated inside the store's transactions and are the race authority
(see the third slice). It follows the [identity retirement](identity-retirement.md)
pattern.

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

## What stayed after the first slice

- Already prepared before this slice: runner claims and idempotent task-token replay
  (`rbit_actions::claims`, selected by the poll through `scheduler/claim_bridge.rs`).
- Still in services or storage:
  - job dependencies and `if:` decisions after each settle (`conditions.rs`, "join"),
    done in the third slice;
  - the poll loop and its version and `Notify` wake-ups, done in the second slice;
  - the sweeper's zombie and timeout passes, done in the second slice;
  - the lease checks of `task_state` and log appends, which answer differently: a
    stopped task is told to cancel, and a closed log has its own message; done in the
    third slice;
  - cancel, rerun and approval request validation, done in the third slice;
  - the review gate and wait timer in SQL, which stay there by design as the race
    authority (third slice).

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

## Poll loop and stale-task passes (second slice)

This slice retires two of the parts listed as remaining above: the poll loop with its
wake-ups, and the sweeper's zombie, endless and abandoned passes. Request validation,
`conditions.rs`, the `task_state` and log lease answers and the review and wait gates are
untouched here; the third slice moved them.

### What moved

| Decision | Legacy place | Now |
| --- | --- | --- |
| Poll admission in order: request validation (its message), `Idempotency-Key` shape, disabled runner, then the instance's `max_polls`; the refusal texts | `scheduler::ActionsService::poll` | `rbit_actions::polling::{admit, PollRefusal}` |
| One poll per runner: a newer poll ends the older, an ended poll keeps the newer one's place, at most `max_polls` runners | `scheduler::{Polls, PollSlot}` | `polling::PollSlots` (generic over the runner id and a `PollEnd` signal; services keep `Polls` as an alias and `PollEnded` over the cancellation token) |
| The long poll: resume a retried key (resend, or end when spent); claim once whenever the scheduler's version moved since the runner's last empty answer; remember the version as idle on nothing; end on a key spent on an unrunnable job; otherwise wait for the version to move or the poll to end | the loop in `poll` | `polling::LongPoll::wait_for_work` over `PollQueue` |
| Stale-task passes in order: zombies (no state call for `zombie_timeout_seconds`) and endless tasks (running longer than `endless_timeout_seconds`) are noted, failed if still running, then closed (masks forgotten, log finalised), settled and their ephemeral runner retired; abandoned jobs (waiting longer than `abandoned_timeout_seconds`) are cancelled in one transaction and each change settled; the reasons; overflow-safe cutoffs | `sweeper::sweep`, `time_out`, `before` | `rbit_actions::stale_tasks::{StaleTasks, Timeouts, before}` over `StaleTaskStore` |

Ports model the storage transitions: `StaleTaskStore::{silent, long_running,
cancel_abandoned, fail}` are the existing SQL (`silent_tasks`, `long_running_tasks`,
`cancel_abandoned`, `finish_task`), unchanged and still the race authority: a task another
writer finished first is not failed again (`fail` answers `None`), and is then not closed,
settled or counted. `PollQueue::wait` is the runtime: the `Notify` is armed before the
version is compared again, so a move after the loop read the version is never missed.
The armed wait now compares the version once more before sleeping, so a move that also
fired the notification is not seen twice; answers are unchanged.

Adapters: `rbit-services/src/actions/scheduler/poll_queue.rs` (`PollSession`: version,
idle version, claim and replay sessions, deadline, replacement, shutdown) and
`rbit-services/src/actions/stale_task_port.rs` (`StaleTaskSession`: the task store, the log
note, masks and finalisation, settling, ephemeral runner deletion and the runner cache).
The sweeper's other passes (logs, runs, files, tokens, gauges) are unchanged.

### Boundary

`scripts/rewrite/actions-poll-sweep-retirement.mjs`, run by `check.mjs`, rejects in the
services, web and server production sources (the two adapters exempt): a poll registry
(`PollSlot`, a `by_runner` map), the poll refusal texts, the version-against-idle claim
decision, calls of the stale-task selections and abandonment, and the stale-task reasons or
a `time_out` function. Against `f043a2df` it reports five violations; against this tree,
none. `scripts/tests/actions-poll-sweep-retirement.test.mjs` pins it.

### Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Same refusal order, texts and codes; same claim and replay calls in the same order; the same stale-task store calls, log notes, failures, settles and runner deletions in the same order, counted the same way. Unchanged suites: `actions_claims`, `actions_runs`, `actions_sweeper`, the runner protocol and long-poll tests, the storage race tests. The scheduler's `polls_are_one_per_runner_and_bounded` and the sweeper's `cutoffs_never_overflow` unit tests run unchanged through the aliases. New: `rbit-actions` `tests/polling.rs` (5) and `tests/stale_tasks.rs` (3). |
| Security | Disabled runners still take nothing; another runner's key still resumes nothing; tasks are failed only by the same SQL guards. |
| Migration and rollback | No schema, migration, route or protocol change; latest migration 0073. |
| Performance | An idle poll still costs no query; one extra atomic read per wait. No release measurement claimed. |

### Results and cost

A quick run of the repository's CI jobs at the code tip (base `f043a2df`) passed rewrite
contracts, workspace-wide formatting and all-target Clippy with `-D warnings`, licenses and
the front-end build; nextest ran `rbit-actions` and `rbit-services`: 830 passed, none
skipped. Storage's race tests (SQL unchanged) and the `rbit-web` suites run in the
integrated full check. Locally, Clippy of both crates was clean apart from the known
Windows-only `large_futures` errors in the services integration target.

One code commit: 10 files, +762 / −126. `rbit-actions` production: 2 new files and
wiring (+308); tests: 2 new files (+271); `rbit-services`: 2 new adapters (+141) and 3
edited files (+42 / −126); checker: a 41-line guard, a 38-line test and two lines in
`check.mjs`. About one session.

### What remained for the row after this slice

The dependency and `if:` decisions after each settle (`conditions.rs`), the `task_state`
and log lease answers, cancel, rerun and approval request validation, and the review gate
and wait timer: all handled by the third slice below.

## Third slice: job decisions, requests, lease answers and the review gate

Built on `f043a2df`, alongside the poll-loop slice above, which owns the poll loop and the
sweeper's zombie and timeout passes; this slice owns the rest of what the first slice left.

| Decision | Legacy place | Now |
| --- | --- | --- |
| A job's own `if:`: true runs, false skips, unevaluable fails | `conditions::evaluate_condition` | `scheduling::jobs::condition_verdict` |
| The contexts an `if:` reads (`github` without any token, `inputs`, `vars`, `matrix`, `strategy`, `needs` and the needs-derived status) | `conditions::condition_contexts` | `jobs::condition_contexts` (services read the parts in the same order) |
| The calls' own `if:`, outermost first, each level with its outer inputs, and the failure message naming the call | `calls::call_conditions` | `jobs::calls_allow`; `jobs::calls_verdict` maps it to skip or fail |
| A job planned from its needs: a failed plan fails it, an empty matrix included (an empty matrix is a plan error, as on GitHub) | `conditions::expand_deferred` | `jobs::deferred_verdict` |
| Fail-fast: failed keys in key order, only with unfinished siblings, only for a real matrix with `fail-fast` on | `conditions::fail_fast` | `jobs::{fail_fast_candidates, fail_fast_applies}` |
| `needs.<id>.result` names | `conditions::result_name`, `calls::result_name` (two copies) | `jobs::result_name` |
| Cancel, rerun and approval rules and messages | `runs.rs` | `scheduling::requests::{check_rerun, check_approval, ALREADY_FINISHED, NO_MATCHING_JOB, NOT_AWAITING_APPROVAL}` |
| Deployment review requests: comment length, selection of the named environments in order (case-insensitive), the reviewer's right per environment, the messages | `environments::review_deployments` | `requests::{check_review_comment, review_selection, NO_LONGER_REVIEWING}` |
| A state report on a task that ended: cancel unless it is a resent final report of a task that was not cancelled; another runner's task is forbidden | `tasks::task_state` | `scheduling::leases::{owned, ended_task_cancels}` |
| A log append after the task ended or its log closed | `logs::append_log` | `leases::{owned, log_open}` (`Refusal::LogClosed`) |

### The review gate and the wait timer

Both are SQL predicates evaluated inside the store's transactions, and they are the race
authority:

- In `settle`, a job about to wait for a runner whose environment has reviewers, and whose
  attempt none approved, waits for review instead, in the same transaction.
- `review_jobs` and `release_reviews` move reviewed jobs.
- The claim query skips a job whose environment's wait timer has not passed since it was
  queued.

No decision about either is made outside those transactions. Moving any of them out would
open the window the transaction closes. What surrounds them is decided in `rbit-actions`:

- who may review, the environment bounds, and the ref gate (`environments`, from the
  secrets row);
- what a review request selects and answers (`requests`, above).

The SQL is kept by design.

### Boundary

`actions-scheduling-retirement.mjs` additionally rejects these in the services, web and
server adapters:

- `Condition::parse`, a `result_name` function, `.fail_fast &&`;
- the run and review request messages (the REST API's per-id wording is allowed);
- the lease answers for another runner and a closed log.

Against the files at `f043a2df` it reports 6 more violations; against this tree, none. The
discovery boundary now skips unit-test modules, which build fixtures by parsing workflows.

### Third-slice results

A quick run at `3ec43bcc` (base `f043a2df`) passed rewrite contracts (with the type
check), workspace-wide formatting and all-target Clippy with `-D warnings`, licenses and
advisories, and the front-end build. Nextest ran the changed packages, `rbit-actions` and
`rbit-services`: 827 tests, 827 passed, none skipped. That set includes, unchanged:

- the condition, call, needs-plan and fail-fast suites (`actions_conditions`, `actions_calls`, `actions_needs_plans`);
- `actions_runs` (cancel, rerun, approval) and the environment review tests;
- `actions_tokens`, and the task state and log append suites.

The five new `rbit-actions` `scheduling::decision_tests` passed. An earlier run (249)
caught a Clippy pedantic lint and a test expectation: an `if:` without a status function
implies `success()`. Both were fixed in one pass. Storage's race tests (SQL unchanged),
`rbit-web` and Windows were not run.

### Third-slice cost

| Measure | Value |
| --- | --- |
| Code commits | 3 |
| `rbit-actions` production | 5 files, +269 / −3 (`scheduling/jobs`, `scheduling/requests`, `scheduling/leases`, wiring) |
| `rbit-services` production | 7 files, +90 / −152 |
| Tests | 1 new file, +115 |
| Checker and policy scripts | +18 / −2 |
