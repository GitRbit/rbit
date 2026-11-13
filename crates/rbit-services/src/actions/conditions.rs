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
//! * A job whose `runs-on`, `strategy`, `container`, `services` or `environment` reads
//!   `needs` was planned as a blocked stand-in (`JobTemplate::deferred`). Once its needs are
//!   done its `if:` is decided as any other's; if it runs, the job is planned from its
//!   workflow at the run's commit with `needs` known, and the jobs that gives (a matrix
//!   from an earlier job's outputs, for instance) take its place. A plan that fails fails
//!   the job, as on GitHub.
//! * Fail-fast: once a matrix job fails, its unfinished siblings (the same job key) are
//!   cancelled unless the job's strategy turned `fail-fast` off.
//!
//! `strategy.max-parallel` and job-level `concurrency` need no decision: the claim query
//! takes no job beyond them (`CLAIM_CANDIDATE` in the run store).
//!
//! The join workstream (J) owns this file.

use rbit_actions::scheduling::jobs::{self, JobState, Verdict};
use rbit_core::{
    Timestamp,
    actions::{Job, JobId, Run, Status},
};
use rbit_storage::sqlite::actions_runs::{Decision, Settled};
use rbit_workflow::{expr::ContextMap, spec::JobTemplate};
use serde_json::{Map, Value};

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
            // One job that cannot be decided now is left to the sweeper
            // (`decide_stragglers`); the others' decisions still count.
            match self.decide_job(*job).await {
                Ok(decided) => followed.extend(decided),
                Err(error) => tracing::warn!(job = job.0, %error, "a job could not be decided; the sweeper tries again"),
            }
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
            // Actions: plans from needs.
            Some(template) if template.deferred => return self.expand_deferred(&run, &job, &template).await,
            // Actions: reusable workflows. The calls' own `if:` first, outermost first.
            Some(template) => match self.calls_decision(&run, &template).await? {
                Some(decision) => decision,
                None => match &template.condition {
                    Some(condition) => self.evaluate_condition(&run, &job, &template, condition).await?,
                    None => Decision::Run,
                },
            },
            None => Decision::Fail,
        };
        if decision != Decision::Run {
            tracing::info!(run = run.id.0, job = %job.key, ?decision, "job condition decided");
        }
        Ok(store.decide(id, decision, Timestamp::now()).await?)
    }

    // Actions: plans from needs.
    /// Decides the stand-in `job` of a job planned from its needs, and puts the jobs its
    /// plan gives in its place.
    async fn expand_deferred(&self, run: &Run, job: &Job, template: &JobTemplate) -> Result<Option<Settled>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let condition = template.condition.as_deref().unwrap_or("success()");
        let decision = self.evaluate_condition(run, job, template, condition).await?;
        if decision != Decision::Run {
            tracing::info!(run = run.id.0, job = %job.key, ?decision, "job condition decided");
            return Ok(store.decide(job.id, decision, Timestamp::now()).await?);
        }
        let planned = self.plan_from_needs(run, job, template).await?;
        if let Some(verdict) = jobs::deferred_verdict(&planned) {
            if let Err(error) = &planned {
                tracing::warn!(run = run.id.0, job = %job.key, %error, "a job cannot be planned from its needs; it fails");
            } else {
                tracing::info!(run = run.id.0, job = %job.key, "a job planned from its needs has an empty matrix; it is skipped");
            }
            return Ok(store.decide(job.id, recorded(verdict), Timestamp::now()).await?);
        }
        let Ok(planned) = planned else { return Ok(None) };
        let rows = super::runs::JobRows::new(&planned);
        let settled = match store.expand(job.id, &rows.jobs(&planned), self.limits().max_run_jobs, Timestamp::now()).await {
            Ok(settled) => settled,
            Err(rbit_storage::StorageError::Conflict(why)) => {
                tracing::warn!(run = run.id.0, job = %job.key, why, "a job planned from its needs is past the run's job limit; it fails");
                return Ok(store.decide(job.id, Decision::Fail, Timestamp::now()).await?);
            }
            Err(error) => return Err(error.into()),
        };
        if settled.is_some() {
            tracing::info!(run = run.id.0, job = %job.key, jobs = planned.len(), "job planned from its needs");
            if let Err(error) = self.announce(run.id).await {
                tracing::warn!(run = run.id.0, %error, "the planned jobs could not be announced");
            }
        }
        Ok(settled)
    }

    /// The jobs `job`'s workflow, read again at the run's commit, plans for it now that its
    /// needs are done.
    async fn plan_from_needs(
        &self,
        run: &Run,
        job: &Job,
        template: &JobTemplate,
    ) -> Result<Result<Vec<JobTemplate>, rbit_workflow::WorkflowError>, ServiceError> {
        let contexts = self.condition_contexts(run, job, template).await?;
        let Some(repo) = self.ctx().database.admin().find_repository(run.repo_id).await? else {
            return Ok(Err(rbit_workflow::WorkflowError::plan("the repository is gone".to_owned(), None)));
        };
        let reader = self.reader(&repo);
        let existing = self.ctx().database.actions_runs().jobs(run.id).await?.len();
        let (sha, path, key, limits) = (run.sha.clone(), run.workflow_path.clone(), job.key.clone(), self.limits());
        self.evaluate(move |handle, deadline| {
            let workflow = super::workflow_files::GitWorkflows::new(handle, &reader, &sha).load(&path, &limits, deadline)?;
            rbit_workflow::plan::plan_deferred(&workflow, &key, &contexts, existing, &limits, deadline)
        })
        .await
    }

    // Actions: reusable workflows.
    /// What the `if:` of the calls around a called job decide: `None` when all of them
    /// let it run (or there are none).
    async fn calls_decision(&self, run: &Run, template: &JobTemplate) -> Result<Option<Decision>, ServiceError> {
        if template.calls.iter().all(|level| level.condition.is_none()) {
            return Ok(None);
        }
        let (github, vars) = self.call_contexts(run).await?;
        let inputs = self.run_inputs(run).await?;
        let outcome = self.call_conditions(run, template, &github, &vars, inputs).await?;
        if let Err(error) = &outcome {
            tracing::warn!(run = run.id.0, %error, "a calling job's condition cannot be evaluated; the job fails");
        }
        Ok(jobs::calls_verdict(&outcome).map(recorded))
    }

    /// The run's own `inputs` (a dispatch's), as a map.
    async fn run_inputs(&self, run: &Run) -> Result<Map<String, Value>, ServiceError> {
        Ok(match self.ctx().database.actions_runs().event(run.id).await?.and_then(|(_, inputs)| inputs) {
            Some(inputs) => serde_json::from_str::<Map<String, Value>>(&inputs).unwrap_or_default(),
            None => Map::new(),
        })
    }

    /// `condition` of `job` against the server's contexts.
    async fn evaluate_condition(&self, run: &Run, job: &Job, template: &JobTemplate, condition: &str) -> Result<Decision, ServiceError> {
        let contexts = self.condition_contexts(run, job, template).await?;
        let (verdict, error) = jobs::condition_verdict(condition, &contexts);
        if let Some(error) = error {
            tracing::warn!(run = run.id.0, job = %job.key, %error, "a job condition cannot be evaluated; the job fails");
        }
        Ok(recorded(verdict))
    }

    /// The contexts a job-level `if:` may read (`rbit_actions::scheduling::jobs::condition_contexts`),
    /// read in the order the server always read them.
    async fn condition_contexts(&self, run: &Run, job: &Job, template: &JobTemplate) -> Result<ContextMap, ServiceError> {
        let mut github = match self.task_context(run, job).await? {
            Value::Object(github) => github,
            _ => Map::new(),
        };
        github.retain(|key, _| !key.eq_ignore_ascii_case("token"));
        let vars: Map<String, Value> =
            self.task_variables(run).await?.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
        // Actions: reusable workflows. A called job's inputs are its workflow's.
        let inputs = self.run_inputs(run).await?;
        let inputs = if template.calls.is_empty() {
            inputs
        } else {
            self.call_inputs(run, template, &github, &vars, inputs).await?.unwrap_or_default()
        };
        let needs = self.needs(run, template).await?;
        let failed_ancestor = self.failed_ancestor(run, template).await?;
        Ok(jobs::condition_contexts(github, inputs, vars, template, needs, failed_ancestor))
    }

    /// Cancels the unfinished matrix siblings of every failed job of run `id` whose strategy
    /// keeps `fail-fast` on.
    async fn fail_fast(&self, id: rbit_core::actions::RunId) -> Result<Option<Settled>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let all = store.jobs(id).await?;
        let states: Vec<JobState<'_, usize>> = all
            .iter()
            .enumerate()
            .map(|(index, job)| JobState {
                id: index,
                key: &job.key,
                failed: job.status == Status::Failure,
                done: job.status.is_done(),
                attempt: job.attempt,
            })
            .collect();
        let mut cancel = Vec::new();
        for candidate in jobs::fail_fast_candidates(&states) {
            let Some(template) = self.template(&all[candidate.first]).await? else { continue };
            if jobs::fail_fast_applies(&template) {
                let key = candidate.key;
                tracing::info!(
                    run = id.0,
                    job = key,
                    cancelled = candidate.unfinished.len(),
                    "a matrix job failed; fail-fast cancels its siblings"
                );
                cancel.extend(candidate.unfinished.iter().map(|&index| all[index].id));
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
            match self.decide_job(job).await {
                Ok(Some(settled)) => self.settled(&settled).await?,
                Ok(None) => {}
                Err(error) => tracing::warn!(job = job.0, %error, "a job could not be decided"),
            }
        }
        Ok(count)
    }
}

/// A verdict as the store records it.
const fn recorded(verdict: Verdict) -> Decision {
    match verdict {
        Verdict::Run => Decision::Run,
        Verdict::Skip => Decision::Skip,
        Verdict::Fail => Decision::Fail,
    }
}
