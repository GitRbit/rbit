# Actions discovery and language acceptance

Acceptance evidence for the `actions.discovery-language` ledger row: workflow
precedence, parser budgets, expressions, matrices and reusable plans, owned by
`rbit-workflow`. It follows the [identity retirement](identity-retirement.md) pattern.

## What "retired" means for this row

The row's target and its recorded source are the same crate. The parser, its YAML and
expression budgets, the expression language, matrix expansion and reusable-workflow
planning were already implemented in `rbit-workflow` and used by every caller: the
server, `rbit-runner` (`exec`, contexts, job execution) and `rbit-web` (runner protocol,
dispatch inputs). No second parser, evaluator or matrix expander exists in `rbit-web` or
`rbit-runner`; `rbit-runner exec` parses one explicitly named file with the crate's own
functions.

What still lived outside the target was the decision layer around them, in
`rbit-services/src/actions`:

| Decision | Legacy place | Now |
| --- | --- | --- |
| Directory precedence (`.rbit`, `.forgejo`, `.github`; first existing wins) | `triggers::WORKFLOW_DIRS`, re-aliased as `tokens::WORKFLOW_DIRECTORIES`; a second copy of the walk in `dispatch::workflow_paths` | `rbit_workflow::discovery::{WORKFLOW_DIRS, Discovery}` |
| File filter (regular/executable, `yml`/`yaml` by `Path::extension`), name order, 64-file cap | `triggers::workflow_paths` and again in `dispatch::workflow_paths` | `Discovery::record`, `is_workflow_file_name`, `MAX_WORKFLOW_FILES` |
| Read budget: deadline before reading, `max_workflow_bytes`, "could not be read" / "larger than N bytes" errors | `triggers::read_workflow_file` | `discovery::load` |
| Discover all / discover one without reading the others | `triggers::{discover, discover_one}` | `discovery::{paths, discover, discover_one}` |
| Workflow display name (blank name or parse error → path) and `path: error` text | `triggers::WorkflowFile`, `triggers::describe` | `discovery::{WorkflowFile, describe}` |
| Which files an event starts (parse errors always reported, unmatched skipped) | `triggers::candidates` | `discovery::evaluate` |
| Run planning: run-unique concurrency groups (`github.run_id`/`run_number`) are dropped for the run and its jobs | `triggers::plan_workflow`, `runs::Plan` | `rbit_workflow::plan::{plan_run, RunPlan}` |
| Dispatch file naming (file name or path in a workflow directory) | `dispatch::WorkflowFileName` | `discovery::WorkflowFileName` |

`rbit-workflow` stays pure: no Git, no async, no rbit dependency. Git reads cross the new
`discovery::WorkflowSource` port; `rbit-services/src/actions/workflow_files.rs`
(`GitWorkflows`) is its only implementation and only lists trees, reads blobs and logs.
The asynchronous page listing (`ActionsService::workflow_paths`) drives the same
`Discovery` steps with asynchronous reads, so it keeps its request-path behaviour and
does not take an evaluation permit.

## Legacy code removed

- `triggers::{WORKFLOW_DIRS, MAX_WORKFLOW_FILES, WorkflowFile, discover, discover_one,
  workflow_paths, read_workflow_file, describe, plan_workflow}`; `triggers::candidates`
  is now a mapping of `discovery::evaluate` into `Candidate`/`Problem`.
- `runs::Plan` (replaced by `RunPlan`), `tokens::WORKFLOW_DIRECTORIES`.
- The duplicate directory walk and file filter in `dispatch::workflow_paths`, and
  `dispatch::WorkflowFileName` with its parser.
- `rbit-web` imports `WORKFLOW_DIRS` from `rbit_workflow::discovery`, not services.

Services still own what is not a workflow-language decision: events, ref/skip-marker
handling, changed-path comparison, trust, `Candidate`/`Problem` and storage.

## Boundary that fails if legacy paths return

`scripts/rewrite/actions-discovery-retirement.mjs`, run by `scripts/rewrite/check.mjs`,
scans production sources of `rbit-services`, `rbit-web`, `rbit-server` and `rbit-actions`
(comments stripped) and rejects: a list or constant of workflow directories, a redefined
`MAX_WORKFLOW_FILES`, a `yml`/`yaml` extension filter, a direct `rbit_workflow::parse`
call, a direct `plan_with`/`concurrency_group` call or import, the
`WORKFLOW_DIRECTORIES` alias, and, under `rbit-services/src/actions`, a redefined
`read_workflow_file`/`discover`/`discover_one`/`plan_workflow` function or
`WorkflowFile`/`WorkflowFileName`/`Plan` type. Single fixture paths in tests, stored
`concurrency_group` fields and `rbit-runner exec` are allowed. Run against the legacy
files at the baseline commit, it reports ten violations; against this tree, none.
`scripts/tests/actions-discovery-retirement.test.mjs` pins both.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Logic moved verbatim: same order, filter, sort, cap, deadline check, error kinds and text (`calls.rs` still matches "could not be read" to hide private repositories). Existing suites listed in Results pass unchanged; the dispatch naming unit test in `dispatch.rs` is unchanged and still passes through the moved type. |
| Security | Untrusted workflow input keeps every budget: size before parse, deadline before read, YAML/expression/matrix limits in the unchanged parser and planner. Trust classification and fork policy are untouched. `hostile_workflows` passes unchanged. |
| Migration and rollback | No schema, migration, stored format or route change. Latest migration remains 0073. Rollback is the previous binary. |
| Performance | Same Git reads in the same order (one tree per directory until one exists, one blob per file); one extra `WorkflowError` clone per unparsable file in `evaluate`. No release measurement claimed. |

## Source under test

The code commits on top of `9b9a00e2`:

1. Decide workflow discovery and run planning in rbit-workflow.
2. Discover workflows through rbit-workflow and drop the service copies.
3. Fail the rewrite check if retired workflow discovery returns.

## Results

Local Windows runs through the shared worktree helper at the third commit. Full
workspace suites and the Linux CI jobs were not run for this slice.

| Gate | Command scope | Result |
| --- | --- | --- |
| Check | `check` of `rbit-workflow`, `rbit-services`, `rbit-web`, all targets | passed, no warnings |
| Clippy `-D warnings` | `rbit-workflow` all targets; `rbit-services` and `rbit-web` libraries, `rbit-services` unit tests, `rbit-web` test targets | passed |
| Clippy `-D warnings` | `rbit-services` integration target | 16 `large_futures` errors in `tests/pulls.rs` (`Fixture::new`, 16,424 bytes); that file and the fixture are untouched by this slice and no discovery code is on its construction path. Not re-run on the baseline, so "pre-existing" is an inference. |
| Formatting | `cargo fmt --all --check` | passed |
| `rbit-workflow` | all targets: 128 unit tests (10 new `discovery::tests`), conformance and golden suites | 130 passed |
| `rbit-services` unit | `actions` filter | 57 passed |
| `rbit-services` integration | `actions` and `hostile_workflows` filters: pushes 12, pulls 8, dispatch 5, planning 1, calls 4, chains 4, conditions 6, needs plans 4, runs 5, tokens 1, hostile workflows 1 and the other Actions modules | 86 passed |
| `rbit-web` | `actions_eval`; integration `actions` filter (pages, job pages, fork runs, API, settings, runner pages, disabled Actions) | 1 + 105 passed, 1 ignored (ignore predates this change) |
| Rewrite policy | `node scripts/rewrite/check.mjs` | 0 violations |
| Script tests | `actions-discovery-retirement`, `identity-retirement`, policy tests | 23 passed |

The first `rbit-web` integration run failed 40 tests with "initial stylesheet must be
embedded; run npm run build": the fresh checkout had no built CSS. With `app.css` and
`deferred.css` built from the same source commit copied in (both are ignored build
outputs), all 105 passed.

New tests: `rbit-workflow` `discovery::tests` (precedence including an existing but
empty first directory, list errors, the file filter and order, the 64-file cap and its
warning hook, the step API, read budgets and error text, deadline before read,
`discover_one` reading only its file, names and `describe`, dispatch file naming,
`evaluate`, run-unique concurrency groups) and
`scripts/tests/actions-discovery-retirement.test.mjs`.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 3 (plus this record and the ledger row) |
| Files changed by code commits | 23 |
| Lines added / removed (all) | 794 / 266 |
| `rbit-workflow` production | 6 files, +375 / −1 (four new modules, `run_plan.rs`, wiring) |
| `rbit-services` and `rbit-web` production | 12 files, +129 / −265 (one new 63-line adapter) |
| Tests | 1 new file, +206 |
| Checker and policy scripts | one new 38-line Node guard, a 44-line test, 2 lines in `check.mjs` |

Tricky parts:

- `rbit-workflow` is promised to be free of Git and async, but one caller (the page
  listing) reads asynchronously on the request path while the others block inside an
  evaluation. A synchronous port plus a small step API (`Discovery`) serves both without
  moving the page listing onto the evaluation semaphore.
- Error text is an interface: `calls.rs` recognises "could not be read" to make a missing
  file look like a repository that may not be called, so the read error text moved
  byte-for-byte.
- The boundary had to tell a second directory list apart from a single fixture path
  in a test module and from the stored `concurrency_group` field.
- The shared build lane was busy with the parallel slices; each compile waited for the
  lease, which dominated wall-clock time (about two and a half hours, of which perhaps
  one hour was code, tests and documentation).

Estimate for similar rows: where, as here, the target crate already holds the core logic
and only a decision layer leaked into services, a row is one session of three code
commits. Rows whose decisions still live in services or SQL with no target-crate
application (`actions.fork-policy`, `actions.secrets`, `actions.scheduling`) are closer to
the identity estimate: ports, an application and adapters first, tens of commits each.
