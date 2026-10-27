//! What the server decides about jobs while a run moves (§12.4 of `docs/design/actions.md`):
//! a job's own `if:` once its needs are done, and `strategy.fail-fast`.
//!
//! * The store keeps a job with its own `if:` blocked until its needs are done and lists it
//!   in `Settled::to_decide`, so no runner can take it before it is decided. Jobs without
//!   one follow GitHub's default `success()` in SQL (see `settle` in the run store).
//! * [`ActionsService::decide_jobs`] evaluates each listed condition with the contexts the
//!   server has: `github`, `inputs`, `vars` (trusted runs only), `matrix`, `strategy` and
//!   `needs`, and the status the status functions read from the needed jobs' results. True
//!   queues the job, false skips it, and a condition that cannot be evaluated fails it, as on
//!   GitHub. `secrets` are never evaluated on the server.
//! * Fail-fast: once a matrix job fails, its unfinished siblings (the same job key) are
//!   cancelled unless the job's strategy turned `fail-fast` off.
//!
//! `strategy.max-parallel` and job-level `concurrency` need no decision: the claim query
//! takes no job beyond them (`CLAIM_CANDIDATE` in the run store).
//!
//! The join workstream (J) owns this file.

use std::collections::BTreeSet;

use rbit_core::{
    Timestamp,
    actions::{Job, JobId, Run, Status},
};
use rbit_storage::sqlite::actions_runs::{Decision, Settled};
use rbit_workflow::{
    expr::{Condition, ContextMap, Status as ExprStatus},
    spec::{JobResult, JobTemplate},
};
use serde_json::{Map, Value, json};

use super::ActionsService;
use crate::ServiceError;

/// Most jobs the sweeper decides in one pass.
pub(super) const SWEEP_DECISIONS: u32 = 64;

impl ActionsService {
    /// Decides what `settled` leaves to the server: every job listed in `to_decide`, then
    /// fail-fast for the run. Returns the transitions this made, for the caller to follow up
    /// in turn (a decision can finish the needs of further jobs).
    pub(super) async fn decide_jobs(&self, settled: &Settled) -> Result<Vec<Settled>, ServiceError> {
        let mut followed = Vec::new();
        for job in &settled.to_decide {
            followed.extend(self.decide_job(*job).await?);
        }
        followed.extend(self.fail_fast(settled.run_id).await?);
        Ok(followed)
    }

    /// Decides the `if:` of blocked job `id`. `None` when someone decided it first.
    pub(super) async fn decide_job(&self, id: JobId) -> Result<Option<Settled>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let Some(job) = store.job(id).await? else { return Ok(None) };
        let Some(run) = store.run(job.run_id).await? else { return Ok(None) };
        let decision = match self.template(&job).await? {
            Some(template) => match &template.condition {
                Some(condition) => self.evaluate_condition(&run, &job, &template, condition).await?,
                None => Decision::Run,
            },
            None => Decision::Fail,
        };
        if decision != Decision::Run {
            tracing::info!(run = run.id.0, job = %job.key, ?decision, "job condition decided");
        }
        Ok(store.decide(id, decision, Timestamp::now()).await?)
    }

    /// `condition` of `job` against the server's contexts.
    async fn evaluate_condition(&self, run: &Run, job: &Job, template: &JobTemplate, condition: &str) -> Result<Decision, ServiceError> {
        let contexts = self.condition_contexts(run, job, template).await?;
        let outcome = Condition::parse(condition).and_then(|condition| condition.evaluate(&contexts));
        Ok(match outcome {
            Ok(true) => Decision::Run,
            Ok(false) => Decision::Skip,
            Err(error) => {
                tracing::warn!(run = run.id.0, job = %job.key, %error, "a job condition cannot be evaluated; the job fails");
                Decision::Fail
            }
        })
    }

    /// The contexts a job-level `if:` may read (§12.4, second row of the table).
    async fn condition_contexts(&self, run: &Run, job: &Job, template: &JobTemplate) -> Result<ContextMap, ServiceError> {
        let database = &self.ctx().database;
        let mut github = match self.task_context(run, job).await? {
            Value::Object(github) => github,
            _ => Map::new(),
        };
        github.retain(|key, _| !key.eq_ignore_ascii_case("token"));
        let inputs = match database.actions_runs().event(run.id).await?.and_then(|(_, inputs)| inputs) {
            Some(inputs) => serde_json::from_str::<Value>(&inputs).unwrap_or_else(|_| json!({})),
            None => json!({}),
        };
        let vars: Map<String, Value> =
            self.task_variables(run).await?.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
        let needs = self.needs(run, template).await?;
        let status = ExprStatus::of_needs(needs.values().map(|needed| needed.result));
        let needs: Map<String, Value> = needs
            .into_iter()
            .map(|(key, needed)| (key, json!({ "result": result_name(needed.result), "outputs": needed.outputs })))
            .collect();
        let strategy = template.strategy;
        Ok(ContextMap::new()
            .with("github", Value::Object(github))
            .with("inputs", inputs)
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
            .with_status(status))
    }

    /// Cancels the unfinished matrix siblings of every failed job of run `id` whose strategy
    /// keeps `fail-fast` on.
    async fn fail_fast(&self, id: rbit_core::actions::RunId) -> Result<Option<Settled>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let jobs = store.jobs(id).await?;
        let failed: BTreeSet<&str> = jobs.iter().filter(|job| job.status == Status::Failure).map(|job| job.key.as_str()).collect();
        let mut cancel = Vec::new();
        for key in failed {
            let siblings: Vec<&Job> = jobs.iter().filter(|job| job.key == key).collect();
            let unfinished: Vec<JobId> = siblings.iter().filter(|job| !job.status.is_done()).map(|job| job.id).collect();
            if unfinished.is_empty() {
                continue;
            }
            let Some(template) = self.template(siblings[0]).await? else { continue };
            if template.strategy.fail_fast && template.strategy.job_total > 1 {
                tracing::info!(run = id.0, job = key, cancelled = unfinished.len(), "a matrix job failed; fail-fast cancels its siblings");
                cancel.extend(unfinished);
            }
        }
        Ok(store.cancel_jobs(id, &cancel, Timestamp::now()).await?)
    }

    /// The stored template of `job`, or `None` when it cannot be read.
    pub(super) async fn template(&self, job: &Job) -> Result<Option<JobTemplate>, ServiceError> {
        let Some(payload) = self.ctx().database.actions_runs().job_payload(job.id).await? else { return Ok(None) };
        Ok(serde_json::from_slice(&payload).ok())
    }

    /// Decides the jobs whose decision was lost (a restart between a transition and its
    /// decision); called by the sweeper.
    pub(super) async fn decide_stragglers(&self) -> Result<usize, ServiceError> {
        let undecided = self.ctx().database.actions_runs().undecided(SWEEP_DECISIONS).await?;
        let count = undecided.len();
        for job in undecided {
            if let Some(settled) = self.decide_job(job).await? {
                self.settled(&settled).await?;
            }
        }
        Ok(count)
    }
}

/// `needs.<id>.result` as written in workflows.
const fn result_name(result: JobResult) -> &'static str {
    match result {
        JobResult::Success => "success",
        JobResult::Failure => "failure",
        JobResult::Cancelled => "cancelled",
        JobResult::Skipped => "skipped",
    }
}
