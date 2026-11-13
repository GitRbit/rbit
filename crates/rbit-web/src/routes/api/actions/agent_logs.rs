//! rbit's own Actions endpoints for agents: read what failed without reading whole logs.
//!
//! * `GET …/actions/runs/{run_id}/failure`: the failure digest (`rbit.failure/1`): per job
//!   that did not succeed, its termination record, its error annotations and a cleaned
//!   excerpt of the step that ended it, with a `more` command for the rest.
//! * `GET …/actions/jobs/{job_id}/steps/{step}/log`: one step's lines (`rbit.log/1`), the
//!   last `tail` (default 100, at most 1,000; `0` gives none, only where the log ends) or
//!   `lines` from line `from`, of attempt
//!   `attempt` (default the current one). Lines are cleaned of timestamps and colours
//!   unless `raw=true`.
//! * `GET …/actions/jobs/{job_id}/annotations`: the annotations of an attempt.
//! * `GET …/actions/jobs/{job_id}/attempts`: every attempt of a job and how each ended, so
//!   "failed, then passed on the same commit" can be seen.
//!
//! Everything is read with the caller's rights, as the GitHub-shaped endpoints are.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use rbit_core::actions::{JobId, RunId, TerminationReason};
use rbit_services::tokens::Right;
use rbit_services::{
    RepoContext,
    actions::logs::{FailedJob, LogRange, MAX_LINES_PER_CALL},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{actions, terminations};
use crate::{
    AppState,
    routes::api::{
        auth::Caller,
        error::{ApiError, ApiResult},
    },
};

/// The default and largest tail a step log answers with.
const DEFAULT_TAIL: usize = 100;
const MAX_TAIL: usize = 1000;
/// The most bytes one step log answer carries.
const MAX_LOG_BYTES: usize = 256 * 1024;

/// The path of these endpoints.
#[derive(Debug, Deserialize)]
pub struct AgentPath {
    owner: String,
    repo: String,
    run_id: Option<String>,
    job_id: Option<String>,
    step: Option<String>,
}

impl AgentPath {
    async fn repo(&self, state: &AppState, caller: &Caller) -> ApiResult<RepoContext> {
        caller.open_repo(state, &self.owner, &self.repo).await
    }

    fn number(value: Option<&str>) -> ApiResult<i64> {
        value.and_then(|value| value.parse::<i64>().ok()).filter(|value| *value > 0).ok_or_else(ApiError::not_found)
    }
}

/// `GET …/actions/runs/{run_id}/failure`
pub async fn failure(State(state): State<AppState>, caller: Caller, Path(path): Path<AgentPath>) -> ApiResult<Json<Value>> {
    caller.require_right(Right::ReadLogs)?;
    let ctx = path.repo(&state, &caller).await?;
    let digest = actions(&state)?.failure_digest(&ctx, RunId(AgentPath::number(path.run_id.as_deref())?)).await?;
    let run = &digest.run;
    let jobs: Vec<Value> = digest.failed.iter().map(failed_job).collect();
    Ok(Json(json!({
        "schema": "rbit.failure/1",
        "run": run.id.0,
        "number": run.number,
        "attempt": run.attempt,
        "sha": run.sha,
        "status": run.status.to_string(),
        // Why a run failed before any job: its workflow could not be read or planned.
        "workflow_error": run.error,
        "jobs": jobs,
        "resolved": digest.resolved,
    })))
}

/// A failed job as the digest, status and watch show it.
pub fn failed_job(failed: &FailedJob) -> Value {
    let job = &failed.job;
    let step = failed.step.as_ref();
    let mut termination = json!({});
    // The digest's step is the one that ended the job even when the record names none.
    let named = if failed.termination.step.is_some() {
        failed.termination
    } else {
        rbit_core::actions::Termination { step: step.and_then(|step| u32::try_from(step.number).ok()), ..failed.termination }
    };
    terminations::describe(&mut termination, named, step.map(|step| (step.number + 1, step.name.as_str())), failed.log_complete);
    let excerpt = failed.excerpt.as_ref().zip(step).map(|(excerpt, step)| {
        let number = step.number + 1;
        let total = usize::try_from(step.log_length).unwrap_or_default();
        let mut value = json!({ "step": number, "lines": excerpt.lines, "shown": excerpt.covered, "total": total });
        if excerpt.covered < total {
            value["truncated"] = json!(true);
            value["more"] = json!(format!("rbit run log {} --step {number} --tail 200", job.id.0));
        }
        value
    });
    let annotations: Vec<Value> = failed
        .annotations
        .iter()
        .map(|annotation| json!({ "path": annotation.path, "line": annotation.line, "message": annotation.message }))
        .collect();
    let tests = failed.tests.as_ref().map(|report| {
        let items: Vec<Value> = report
            .items
            .iter()
            .map(|test| {
                let flaky = failed.flaky.contains(&test.id);
                json!({ "id": test.id, "path": test.path, "line": test.line, "message": test.message, "flaky": flaky })
            })
            .collect();
        json!({ "total": report.total, "failed": report.failed, "items": items, "truncated": report.failed > items.len() as u64 })
    });
    let fingerprints: Vec<&str> = failed.faults.iter().map(|fault| fault.fingerprint.as_str()).collect();
    json!({
        "job": job.name,
        "job_id": job.id.0,
        "attempt": job.attempt,
        "conclusion": if failed.termination.reason == TerminationReason::Timeout { "timed_out".to_owned() } else { job.status.to_string() },
        "termination": termination["termination"],
        "tests": tests,
        "annotations": annotations,
        "excerpt": excerpt,
        "seen": failed.seen,
        "also_failing_on_base": failed.also_failing_on_base,
        "fingerprints": fingerprints,
    })
}

/// The query of a step log.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StepLogQuery {
    tail: Option<usize>,
    from: Option<u64>,
    lines: Option<usize>,
    attempt: Option<i64>,
    raw: bool,
}

/// `GET …/actions/jobs/{job_id}/steps/{step}/log`
pub async fn step_log(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<AgentPath>,
    Query(query): Query<StepLogQuery>,
) -> ApiResult<Json<Value>> {
    caller.require_right(Right::ReadLogs)?;
    let ctx = path.repo(&state, &caller).await?;
    let job = JobId(AgentPath::number(path.job_id.as_deref())?);
    let number = AgentPath::number(path.step.as_deref())?;
    if query.tail.is_some() && query.from.is_some() {
        return Err(ApiError::invalid("tail", "give `tail` or `from`, not both"));
    }
    let actions = actions(&state)?;
    let (task, _) = actions.job_attempt(&ctx, job, query.attempt).await?;
    let (range, limit) = match query.from {
        Some(from) => (LogRange::From(from), query.lines.unwrap_or(DEFAULT_TAIL)),
        None => (LogRange::Tail, query.tail.unwrap_or(DEFAULT_TAIL)),
    };
    let limit = limit.clamp(1, MAX_TAIL.min(MAX_LINES_PER_CALL));
    let mut window = actions.step_log(&ctx, task, number - 1, range, limit, MAX_LOG_BYTES).await?;
    // `tail=0` asks for no line: only where the log stands (the read takes at least one).
    if query.from.is_none() && query.tail == Some(0) {
        window.more_before |= !window.lines.is_empty();
        window.lines.clear();
        window.first_line = window.next_line;
    }
    let lines: Vec<String> =
        if query.raw { window.lines } else { window.lines.iter().map(|line| rbit_services::actions::logs::readable_line(line)).collect() };
    Ok(Json(json!({
        "schema": "rbit.log/1",
        "job_id": job.0,
        "step": number,
        "first_line": window.first_line,
        "lines": lines,
        "next_line": window.next_line,
        "more_before": window.more_before,
        "live": window.live,
        "truncated": window.truncated,
        "expired": window.expired,
    })))
}

/// The query of a job's annotations.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AttemptQuery {
    attempt: Option<i64>,
}

/// `GET …/actions/jobs/{job_id}/annotations`
pub async fn annotations(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<AgentPath>,
    Query(query): Query<AttemptQuery>,
) -> ApiResult<Json<Value>> {
    // Annotations are lines of the log: a token limited to other rights reads none.
    caller.require_right(Right::ReadLogs)?;
    let ctx = path.repo(&state, &caller).await?;
    let job = JobId(AgentPath::number(path.job_id.as_deref())?);
    let stored = actions(&state)?.job_annotations(&ctx, job, query.attempt).await?;
    let items: Vec<Value> = stored
        .iter()
        .map(|annotation| {
            let text = rbit_services::actions::annotations::annotation_text(&annotation.text);
            json!({ "level": annotation.level, "path": text.file, "line": text.line, "message": text.message, "log_line": annotation.line })
        })
        .collect();
    Ok(Json(json!({ "schema": "rbit.annotations/1", "job_id": job.0, "annotations": items })))
}

/// `GET …/actions/jobs/{job_id}/attempts`
pub async fn attempts(State(state): State<AppState>, caller: Caller, Path(path): Path<AgentPath>) -> ApiResult<Json<Value>> {
    let ctx = path.repo(&state, &caller).await?;
    let job = JobId(AgentPath::number(path.job_id.as_deref())?);
    let attempts = actions(&state)?.job_attempts(&ctx, job).await?;
    let items: Vec<Value> = attempts
        .iter()
        .map(|(attempt, current, end)| {
            let reason = end.and_then(|end| end.termination).map(|termination| termination.reason.as_str());
            json!({ "attempt": attempt, "current": current, "reason": reason, "log_complete": end.is_some_and(|end| end.log_complete) })
        })
        .collect();
    Ok(Json(json!({ "schema": "rbit.attempts/1", "job_id": job.0, "attempts": items })))
}
