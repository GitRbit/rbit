//! A workflow → the [`crate::spec::JobTemplate`]s of one run: matrices expanded, and the fields the
//! server decides when a run is created evaluated (§12.4 of `docs/design/actions.md`,
//! first row).
//!
//! # What planning evaluates
//!
//! With the run's `github`, `inputs` and `vars` ([`RunContext`]): the matrix itself
//! (`${{ fromJSON(inputs.matrix) }}` and expressions inside it), `strategy.fail-fast` and
//! `strategy.max-parallel`. Then, per matrix combination, with `matrix` and `strategy` as
//! well: the job's `name`, `runs-on`, `timeout-minutes` (default
//! [`crate::spec::DEFAULT_TIMEOUT_MINUTES`], capped by [`crate::limits::Limits::max_timeout_minutes`]),
//! `continue-on-error`, job `concurrency`, `container.image` and `services.*.image`. The
//! workflow's `run-name` and `concurrency` are evaluated with the run's contexts too
//! ([`run_name`], [`concurrency_group`]).
//!
//! Everything else stays a template string: the job's `if` for the server when the job
//! unblocks, the rest for the runner. The `strategy` context is
//! `{fail-fast, job-index, job-total, max-parallel}`, `max-parallel` defaulting to the
//! number of the matrix's jobs.
//!
//! # Matrices
//!
//! Expansion follows act's `GetMatrixes` (which Forgejo and Gitea run): the cartesian
//! product of the dimensions, minus every combination an `exclude` entry matches; then each
//! `include` entry is merged into every combination whose *original* values it does not
//! contradict (values it adds may overwrite each other), and an entry that fits none
//! becomes a combination of its own. Combinations are then ordered by their display name
//! `(value, value)` — values in key order — as Forgejo orders them, and a job without a
//! `name` is called `<id> (<values>)`.
//!
//! # Reusable workflows
//!
//! A job that calls a workflow (`uses`) becomes the jobs of that workflow, planned here with
//! [`plan_with`] and a [`Loader`] that finds the workflow: keyed `caller/job` (with the
//! caller's matrix, `caller#n/job`), named `caller / job`, and waiting for what the caller
//! waits for. Each keeps the calls around it ([`crate::spec::CallLevel`]): the caller's `if`, its matrix,
//! the inputs known now and the `with:` that read `needs`, which the server evaluates when
//! the job is handed out, and the secrets it passes on. Inputs are typed and checked against
//! `on.workflow_call` (a missing required one, or an unknown one, fails planning), and so are
//! the secrets a caller names. The called workflow sees the caller's `github` and `vars`, its
//! own `inputs` and `env`. Permissions only narrow: a called job may not ask for more than its
//! caller allows. Calls nest at most [`MAX_CALL_DEPTH`] levels deep and name at most
//! [`MAX_CALLED_WORKFLOWS`] different workflows, GitHub's limits. A job that needs a calling
//! job waits for all of its jobs; its `needs` context reads the called workflow's `outputs`
//! ([`crate::spec::NeedSource`]).

mod calls;
mod contexts;
mod dependencies;
mod evaluation;
mod group_context;
mod loader;
mod matrix;
mod names;
mod planning;
mod run_plan;
mod templates;

pub use self::contexts::RunContext;
pub use self::group_context::GroupContext;
pub use self::loader::{Loaded, Loader, MAX_CALL_DEPTH, MAX_CALLED_WORKFLOWS, NoCalls};
pub use self::matrix::{Axes, Combination, combinations};
pub use self::names::{concurrency_group, run_name};
pub use self::planning::{MAX_JOB_KEY_BYTES, plan, plan_deferred, plan_with};
pub use self::run_plan::{RunPlan, plan_run};
pub(crate) use self::templates::step_template;

#[cfg(test)]
mod tests;
