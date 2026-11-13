//! Ordered recursive workflow/job expansion, admission limits and deferred completion.
use super::calls::{call_inputs, narrowed, passed_secrets};
use super::contexts::{JobContexts, RunContext};
use super::dependencies::{deferred_template, reads_matrix, reads_needs, strategy_reads_needs};
use super::evaluation::Evaluator;
use super::loader::{Loader, MAX_CALL_DEPTH, MAX_CALLED_WORKFLOWS, NoCalls};
use super::matrix::{Combination, expand, matrix_name};
use super::templates::{job_template, strategy_context};
use crate::error::WorkflowError;
use crate::expr::ContextMap;
use crate::limits::{Deadline, Limits};
use crate::model::{Job, JobCall, Permissions, Workflow};
use crate::spec::{CallLevel, JobTemplate, NeedSource, Strategy};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The longest job key the database keeps.
pub const MAX_JOB_KEY_BYTES: usize = 100;

/// Plans every job of `workflow`, in file order, each job's combinations in Forgejo's order.
/// A job that calls a workflow fails planning: see [`plan_with`].
pub fn plan(workflow: &Workflow, context: &RunContext, limits: &Limits, deadline: &Deadline) -> Result<Vec<JobTemplate>, WorkflowError> {
    plan_with(workflow, context, limits, deadline, &mut NoCalls)
}

// Actions: reusable workflows.
/// [`plan`], with the workflows that jobs call found by `loader` and planned in their place.
pub fn plan_with(
    workflow: &Workflow,
    context: &RunContext,
    limits: &Limits,
    deadline: &Deadline,
    loader: &mut dyn Loader,
) -> Result<Vec<JobTemplate>, WorkflowError> {
    let mut planner = Planner { limits, deadline, loader, called: BTreeSet::new(), jobs: 0 };
    let scope = CallScope {
        prefix: String::new(),
        origin: None,
        depth: 1,
        calls: Vec::new(),
        cap: None,
        root_needs: Vec::new(),
        default_ceiling: false,
    };
    Ok(planner.workflow(workflow, &context.contexts(), &scope)?.0)
}

/// Where a workflow's jobs are planned: inside which calls.
struct CallScope {
    /// The key of the calling job (`call`, `outer/inner`); empty for the run's own workflow.
    prefix: String,
    origin: Option<String>,
    depth: usize,
    calls: Vec<CallLevel>,
    /// The most the calling jobs allow.
    cap: Option<Permissions>,
    /// What the calling job waits for, so every job of the called workflow waits for it too.
    root_needs: Vec<String>,
    /// No calling job declared `permissions:`: the jobs inside stay within the repository's
    /// default ([`JobTemplate::default_ceiling`]).
    default_ceiling: bool,
}

impl CallScope {
    fn key(&self, id: &str) -> String {
        if self.prefix.is_empty() { id.to_owned() } else { format!("{}/{id}", self.prefix) }
    }
}

struct Planner<'a> {
    limits: &'a Limits,
    deadline: &'a Deadline,
    loader: &'a mut dyn Loader,
    /// The different workflows called so far.
    called: BTreeSet<String>,
    /// Jobs planned so far, for [`Limits::max_run_jobs`].
    jobs: usize,
}

impl Planner<'_> {
    /// The jobs of `workflow` in `scope`, in file order, and what each of its jobs stands for
    /// in a `needs` context.
    fn workflow(
        &mut self,
        workflow: &Workflow,
        run: &ContextMap,
        scope: &CallScope,
    ) -> Result<(Vec<JobTemplate>, BTreeMap<String, NeedSource>), WorkflowError> {
        let mut sources: BTreeMap<String, NeedSource> = BTreeMap::new();
        let mut planned: BTreeMap<String, Vec<JobTemplate>> = BTreeMap::new();
        for job in needs_order(workflow) {
            self.deadline.check()?;
            let need_names: BTreeMap<String, NeedSource> = job
                .needs
                .iter()
                .map(|need| (need.clone(), sources.get(need).cloned().unwrap_or_else(|| NeedSource::Jobs(vec![scope.key(need)]))))
                .collect();
            let mut needs: Vec<String> = need_names.values().flat_map(NeedSource::keys).collect();
            if job.needs.is_empty() {
                needs.extend(scope.root_needs.iter().cloned());
            }
            // Actions: plans from needs. Planned once its needs are done (`plan_deferred`).
            if job.call.is_some() && strategy_reads_needs(job) {
                return Err(WorkflowError::plan(
                    format!("job `{}`: a job that calls a workflow cannot read `needs` in its `strategy` yet", job.id),
                    Some(job.at),
                ));
            }
            if job.call.is_none() && reads_needs(job) {
                if !scope.calls.is_empty() {
                    return Err(WorkflowError::plan(
                        format!("job `{}`: a called workflow's `runs-on`, matrix or containers cannot read `needs` yet", job.id),
                        Some(job.at),
                    ));
                }
                let key = scope.key(&job.id);
                planned.insert(job.id.clone(), vec![deferred_template(job, &key, &needs)]);
                sources.insert(job.id.clone(), NeedSource::Jobs(vec![key]));
                self.jobs += 1;
                continue;
            }
            let combinations = match &job.strategy.matrix {
                Some(matrix) => expand(matrix, run, &job.id, self.limits, self.deadline)?,
                None => vec![Combination::new()],
            };
            let evaluator = Evaluator { job, contexts: run };
            let fail_fast =
                job.strategy.fail_fast.as_deref().map(|text| evaluator.flag("strategy.fail-fast", text)).transpose()?.unwrap_or(true);
            let max_parallel =
                job.strategy.max_parallel.as_deref().map(|text| evaluator.whole_number("strategy.max-parallel", text)).transpose()?;
            let job_total = u32::try_from(combinations.len()).unwrap_or(u32::MAX);
            let expansion = Expansion { combinations, fail_fast, max_parallel, job_total, need_names, needs };
            let (templates, source) = match &job.call {
                None => self.steps_job(workflow, job, run, scope, expansion)?,
                Some(call) => self.call_job(workflow, job, call, run, scope, expansion)?,
            };
            sources.insert(job.id.clone(), source);
            planned.insert(job.id.clone(), templates);
        }
        let mut all = Vec::new();
        for job in &workflow.jobs {
            all.extend(planned.remove(&job.id).unwrap_or_default());
        }
        Ok((all, sources))
    }

    /// The templates of a job that runs steps, one per matrix combination.
    fn steps_job(
        &mut self,
        workflow: &Workflow,
        job: &Job,
        run: &ContextMap,
        scope: &CallScope,
        expansion: Expansion,
    ) -> Result<(Vec<JobTemplate>, NeedSource), WorkflowError> {
        let Expansion { combinations, fail_fast, max_parallel, job_total, need_names, needs } = expansion;
        let mut templates = Vec::new();
        if self.jobs + combinations.len() > self.limits.max_run_jobs {
            return Err(WorkflowError::limit(format!("a run may have at most {} jobs", self.limits.max_run_jobs), Some(job.at)));
        }
        let key = scope.key(&job.id);
        if key.len() > MAX_JOB_KEY_BYTES {
            return Err(WorkflowError::plan(format!("job `{key}`: calls nest too deep for its key"), Some(job.at)));
        }
        let permissions = narrowed(job.permissions.or(workflow.permissions), scope.cap, &job.id, job.at)?;
        for (job_index, combination) in (0..).zip(combinations) {
            self.deadline.check()?;
            let strategy = Strategy { fail_fast, max_parallel, job_index, job_total };
            let mut template = job_template(workflow, job, run, combination, strategy, self.limits)?;
            template.job_id.clone_from(&key);
            template.needs.clone_from(&needs);
            template.permissions = permissions.map(Into::into);
            if !scope.calls.is_empty() || need_names.values().any(|source| matches!(source, NeedSource::Call { .. })) {
                template.need_names.clone_from(&need_names);
            }
            if !scope.calls.is_empty() {
                let callers: Vec<&str> = scope.calls.iter().map(|call| call.name.as_str()).collect();
                template.name = format!("{} / {}", callers.join(" / "), template.name);
                template.calls.clone_from(&scope.calls);
                template.default_ceiling = scope.default_ceiling;
            }
            templates.push(template);
            self.jobs += 1;
        }
        Ok((templates, NeedSource::Jobs(vec![key])))
    }

    /// The templates of the called workflow's jobs, for each matrix combination of the job
    /// that calls it.
    fn call_job(
        &mut self,
        workflow: &Workflow,
        job: &Job,
        call: &JobCall,
        run: &ContextMap,
        scope: &CallScope,
        expansion: Expansion,
    ) -> Result<(Vec<JobTemplate>, NeedSource), WorkflowError> {
        let Expansion { combinations, fail_fast, max_parallel, job_total, need_names, needs } = expansion;
        let matrix_job = job.strategy.matrix.is_some();
        let mut templates = Vec::new();
        if scope.depth >= MAX_CALL_DEPTH {
            return Err(WorkflowError::plan(
                format!("job `{}`: workflows call each other more than {MAX_CALL_DEPTH} levels deep", job.id),
                Some(job.at),
            ));
        }
        let (mut keys, mut jobs, mut outputs) = (Vec::new(), BTreeMap::new(), BTreeMap::new());
        for (job_index, combination) in (0..).zip(combinations) {
            self.deadline.check()?;
            let strategy = Strategy { fail_fast, max_parallel, job_index, job_total };
            let contexts = JobContexts { run, matrix: Value::Object(combination.clone()), strategy: strategy_context(strategy) };
            let evaluator = Evaluator { job, contexts: &contexts };
            let name = match &job.name {
                Some(name) => evaluator.text("name", name)?,
                None => String::new(),
            };
            let name = match name.trim() {
                "" if combination.is_empty() => job.id.clone(),
                "" => format!("{} {}", job.id, matrix_name(&combination)),
                _ => name,
            };
            let loaded = self.loader.load(scope.origin.as_deref(), &call.target).map_err(|problem| evaluator.refuse("uses", &problem))?;
            self.called.insert(loaded.id.clone());
            if self.called.len() > MAX_CALLED_WORKFLOWS {
                return Err(evaluator.refuse("uses", &format!("a run may call at most {MAX_CALLED_WORKFLOWS} different workflows")));
            }
            let Some(declared) = &loaded.workflow.on.workflow_call else {
                return Err(evaluator.refuse("uses", &format!("`{}` does not run on `workflow_call`", call.uses)));
            };
            let (inputs, deferred) = call_inputs(&evaluator, &call.with, declared)?;
            let secrets = passed_secrets(&evaluator, &call.secrets, declared)?;
            let level = CallLevel {
                name,
                condition: job.condition.clone(),
                matrix: combination.clone(),
                strategy,
                needs: need_names.clone(),
                inputs: inputs.clone(),
                deferred,
                secrets,
            };
            let prefix = if matrix_job { scope.key(&format!("{}#{job_index}", job.id)) } else { scope.key(&job.id) };
            let mut calls = scope.calls.clone();
            calls.push(level);
            let cap = narrowed(job.permissions.or(workflow.permissions), scope.cap, &job.id, job.at)?;
            let inner = CallScope {
                prefix,
                origin: Some(loaded.origin.clone()),
                depth: scope.depth + 1,
                calls,
                cap,
                root_needs: needs.clone(),
                default_ceiling: scope.default_ceiling || cap.is_none(),
            };
            let inner_run = run.clone().with("inputs", inputs);
            let (inner_templates, inner_sources) = self.workflow(&loaded.workflow, &inner_run, &inner)?;
            keys.extend(inner_templates.iter().map(|template| template.job_id.clone()));
            for (id, source) in inner_sources {
                match (jobs.get_mut(&id), source) {
                    (Some(NeedSource::Jobs(known)), NeedSource::Jobs(more)) => known.extend(more),
                    (_, source) => {
                        jobs.insert(id, source);
                    }
                }
            }
            outputs.clone_from(&declared.outputs);
            templates.extend(inner_templates);
        }
        keys.dedup();
        Ok((templates, NeedSource::Call { keys, outputs, jobs }))
    }
}

/// What the planned copies of one job share: its matrix combinations, its strategy and what
/// its `needs` stand for.
struct Expansion {
    combinations: Vec<Combination>,
    fail_fast: bool,
    max_parallel: Option<u32>,
    job_total: u32,
    need_names: BTreeMap<String, NeedSource>,
    needs: Vec<String>,
}

/// Plans job `job_id` of the run's own `workflow` now that its needs are done: `run` holds
/// the run's contexts and `needs`, with their results and outputs. `existing_jobs` is how
/// many jobs the run already has, for [`Limits::max_run_jobs`].
pub fn plan_deferred(
    workflow: &Workflow,
    job_id: &str,
    run: &ContextMap,
    existing_jobs: usize,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Vec<JobTemplate>, WorkflowError> {
    let job = workflow
        .jobs
        .iter()
        .find(|job| job.id == job_id)
        .ok_or_else(|| WorkflowError::plan(format!("the workflow has no job `{job_id}` any more"), None))?;
    let mut no_calls = NoCalls;
    let mut planner = Planner { limits, deadline, loader: &mut no_calls, called: BTreeSet::new(), jobs: existing_jobs.saturating_sub(1) };
    let scope = CallScope {
        prefix: String::new(),
        origin: None,
        depth: 1,
        calls: Vec::new(),
        cap: None,
        root_needs: Vec::new(),
        default_ceiling: false,
    };
    let need_names: BTreeMap<String, NeedSource> =
        job.needs.iter().map(|need| (need.clone(), NeedSource::Jobs(vec![need.clone()]))).collect();
    let combinations = match &job.strategy.matrix {
        Some(matrix) => expand(matrix, run, &job.id, limits, deadline)?,
        None => vec![Combination::new()],
    };
    let evaluator = Evaluator { job, contexts: run };
    let fail_fast = job.strategy.fail_fast.as_deref().map(|text| evaluator.flag("strategy.fail-fast", text)).transpose()?.unwrap_or(true);
    let max_parallel =
        job.strategy.max_parallel.as_deref().map(|text| evaluator.whole_number("strategy.max-parallel", text)).transpose()?;
    let job_total = u32::try_from(combinations.len()).unwrap_or(u32::MAX);
    let expansion = Expansion { combinations, fail_fast, max_parallel, job_total, need_names, needs: job.needs.clone() };
    let (mut templates, _) = planner.steps_job(workflow, job, run, &scope, expansion)?;
    // The stand-in's decision stands for the planned jobs, unless their `if:` reads the
    // matrix they only now have: then each decides its own.
    // A stand-in that ran past a failed need (`if: failure()`) must not be gated by
    // `success()` again: its planned jobs are decided with `always()`.
    for template in &mut templates {
        if !template.condition.as_deref().is_some_and(reads_matrix) {
            template.condition = Some("always()".to_owned());
        }
    }
    Ok(templates)
}

/// The jobs of `workflow`, each after those it needs, otherwise in file order. Parsing
/// refused cycles; a job left on one would come last.
fn needs_order(workflow: &Workflow) -> Vec<&Job> {
    let mut placed: BTreeSet<&str> = BTreeSet::new();
    let mut order = Vec::new();
    while order.len() < workflow.jobs.len() {
        let before = order.len();
        for job in &workflow.jobs {
            if !placed.contains(job.id.as_str()) && job.needs.iter().all(|need| placed.contains(need.as_str())) {
                placed.insert(&job.id);
                order.push(job);
            }
        }
        if order.len() == before {
            order.extend(workflow.jobs.iter().filter(|job| !placed.contains(job.id.as_str())));
            break;
        }
    }
    order
}
