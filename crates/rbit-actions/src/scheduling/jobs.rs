//! What the server decides about jobs after their needs settle (§12.4 of the Actions
//! design): a job's own `if:`, the `if:` of the calls around a called job, a job planned
//! from its needs, and `strategy.fail-fast`. The store keeps a job with its own `if:`
//! blocked until it is decided, so no runner takes it before; the decision itself is
//! recorded by the store's transition, the race authority.
//!
//! * A condition that is true queues the job, false skips it, and one that cannot be
//!   evaluated fails it, as on GitHub ([`condition_verdict`]). A called job runs only when
//!   every call around it lets it ([`calls_verdict`]).
//! * The contexts an `if:` reads are `github` (never a token), `inputs`, `vars` (trusted
//!   runs only: the adapter passes none otherwise), `matrix`, `strategy` and `needs`, with
//!   the status functions reading the needed jobs' results ([`condition_contexts`]).
//!   `secrets` are never evaluated on the server.
//! * A job planned from its needs whose matrix is empty is skipped; one that cannot be
//!   planned, or would pass the run's job limit, fails ([`deferred_verdict`]).
//! * Once a matrix job fails, its unfinished siblings (same job key) are cancelled unless
//!   the job's strategy turned `fail-fast` off ([`fail_fast_candidates`], [`fail_fast_applies`]).
use std::collections::{BTreeMap, BTreeSet};

use rbit_workflow::{
    expr::{Condition, ContextMap, Status},
    spec::{CallLevel, JobResult, JobTemplate, Needed},
};
use serde_json::{Map, Value, json};

/// What becomes of a job waiting for a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It waits for a runner.
    Run,
    /// It is skipped, as are the jobs that need it to succeed.
    Skip,
    /// It fails.
    Fail,
}

/// `condition` against `contexts`: the verdict, and why it failed when it cannot be
/// evaluated (for the adapter's log).
pub fn condition_verdict(condition: &str, contexts: &ContextMap) -> (Verdict, Option<String>) {
    match Condition::parse(condition).and_then(|condition| condition.evaluate(contexts)) {
        Ok(true) => (Verdict::Run, None),
        Ok(false) => (Verdict::Skip, None),
        Err(error) => (Verdict::Fail, Some(error.to_string())),
    }
}

/// The `if:` of the calls around a called job, outermost first: `Ok(false)` once one says
/// no, `Err` naming the call when one cannot be evaluated. Each level is evaluated with
/// `contexts`, given the inputs of the level around it (the run's for the outermost).
pub fn calls_allow(
    levels: &[CallLevel],
    run_inputs: Map<String, Value>,
    mut contexts: impl FnMut(&CallLevel, &Map<String, Value>) -> ContextMap,
) -> Result<bool, String> {
    let mut outer = run_inputs;
    for level in levels {
        if let Some(condition) = &level.condition {
            let contexts = contexts(level, &outer);
            match Condition::parse(condition).and_then(|condition| condition.evaluate(&contexts)) {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(error) => return Err(format!("`{}`: `if`: {error}", level.name)),
            }
        }
        outer.clone_from(&level.inputs);
    }
    Ok(true)
}

/// What the `if:` of the calls around a called job decide: `None` when all of them let it
/// run; a call condition that cannot be evaluated fails the job.
pub const fn calls_verdict<E>(outcome: &Result<bool, E>) -> Option<Verdict> {
    match outcome {
        Ok(true) => None,
        Ok(false) => Some(Verdict::Skip),
        Err(_) => Some(Verdict::Fail),
    }
}

/// The verdict on a job planned from its needs, when it is not to run as planned: an
/// empty matrix skips it, a plan that failed fails it.
pub fn deferred_verdict<T, E>(planned: &Result<Vec<T>, E>) -> Option<Verdict> {
    match planned {
        Ok(planned) if planned.is_empty() => Some(Verdict::Skip),
        Ok(_) => None,
        Err(_) => Some(Verdict::Fail),
    }
}

/// The contexts a job-level `if:` may read (§12.4, second row of the table). `github` is
/// the task's context; any `token` key is dropped. `inputs` are the run's, or the called
/// workflow's for a called job. `failed_ancestor`: a job further up the needs failed.
pub fn condition_contexts(
    mut github: Map<String, Value>,
    inputs: Map<String, Value>,
    vars: Map<String, Value>,
    template: &JobTemplate,
    needs: BTreeMap<String, Needed>,
    failed_ancestor: bool,
) -> ContextMap {
    github.retain(|key, _| !key.eq_ignore_ascii_case("token"));
    let status = Status::of_needs(needs.values().map(|needed| needed.result)).with_failed_ancestor(failed_ancestor);
    let needs: Map<String, Value> =
        needs.into_iter().map(|(key, needed)| (key, json!({ "result": result_name(needed.result), "outputs": needed.outputs }))).collect();
    let strategy = template.strategy;
    ContextMap::new()
        .with("github", Value::Object(github))
        .with("inputs", Value::Object(inputs))
        .with("vars", Value::Object(vars))
        .with("matrix", Value::Object(template.matrix.clone()))
        .with(
            "strategy",
            json!({
                "fail-fast": strategy.fail_fast,
                "job-index": strategy.job_index,
                "job-total": strategy.job_total,
                "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
            }),
        )
        .with("needs", Value::Object(needs))
        .with_status(status)
}

/// `needs.<id>.result` as written in workflows.
pub const fn result_name(result: JobResult) -> &'static str {
    match result {
        JobResult::Success => "success",
        JobResult::Failure => "failure",
        JobResult::Cancelled => "cancelled",
        JobResult::Skipped => "skipped",
    }
}

/// A job of a run as fail-fast reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobState<'a, Id> {
    pub id: Id,
    pub key: &'a str,
    pub failed: bool,
    pub done: bool,
}

/// A failed job's matrix with siblings still unfinished: whose template to read (the
/// first sibling's) and which jobs fail-fast would cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailFast<'a, Id> {
    pub key: &'a str,
    pub first: Id,
    pub unfinished: Vec<Id>,
}

/// The failed job keys of a run, by key, that still have unfinished siblings.
pub fn fail_fast_candidates<'a, Id: Copy>(jobs: &[JobState<'a, Id>]) -> Vec<FailFast<'a, Id>> {
    let failed: BTreeSet<&str> = jobs.iter().filter(|job| job.failed).map(|job| job.key).collect();
    failed
        .into_iter()
        .filter_map(|key| {
            let siblings: Vec<&JobState<'a, Id>> = jobs.iter().filter(|job| job.key == key).collect();
            let unfinished: Vec<Id> = siblings.iter().filter(|job| !job.done).map(|job| job.id).collect();
            (!unfinished.is_empty()).then(|| FailFast { key: siblings[0].key, first: siblings[0].id, unfinished })
        })
        .collect()
}

/// Whether a failed matrix job cancels its siblings: `fail-fast` on, and a real matrix.
pub const fn fail_fast_applies(template: &JobTemplate) -> bool {
    template.strategy.fail_fast && template.strategy.job_total > 1
}
