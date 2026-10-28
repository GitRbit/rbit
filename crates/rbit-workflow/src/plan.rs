//! A workflow → the [`JobTemplate`]s of one run: matrices expanded, and the fields the
//! server decides when a run is created evaluated (§12.4 of `docs/design/actions.md`,
//! first row).
//!
//! # What planning evaluates
//!
//! With the run's `github`, `inputs` and `vars` ([`RunContext`]): the matrix itself
//! (`${{ fromJSON(inputs.matrix) }}` and expressions inside it), `strategy.fail-fast` and
//! `strategy.max-parallel`. Then, per matrix combination, with `matrix` and `strategy` as
//! well: the job's `name`, `runs-on`, `timeout-minutes` (default
//! [`DEFAULT_TIMEOUT_MINUTES`], capped by [`Limits::max_timeout_minutes`]),
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
//! waits for. Each keeps the calls around it ([`CallLevel`]): the caller's `if`, its matrix,
//! the inputs known now and the `with:` that read `needs`, which the server evaluates when
//! the job is handed out, and the secrets it passes on. Inputs are typed and checked against
//! `on.workflow_call` (a missing required one, or an unknown one, fails planning), and so are
//! the secrets a caller names. The called workflow sees the caller's `github` and `vars`, its
//! own `inputs` and `env`. Permissions only narrow: a called job may not ask for more than its
//! caller allows. Calls nest at most [`MAX_CALL_DEPTH`] levels deep and name at most
//! [`MAX_CALLED_WORKFLOWS`] different workflows, GitHub's limits. A job that needs a calling
//! job waits for all of its jobs; its `needs` context reads the called workflow's `outputs`
//! ([`NeedSource`]).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::{
    error::{Location, WorkflowError},
    expr::{ContextMap, Contexts, Template, to_number, to_text},
    limits::{Deadline, Limits},
    model::{CallInputKind, CallSecrets, CallTarget, Job, JobCall, Matrix, Permission, Permissions, Step, Workflow},
    parse::call_input_value,
    spec::{
        CallLevel, ConcurrencyGroup, Container, DEFAULT_TIMEOUT_MINUTES, DeferredInput, JOB_TEMPLATE_VERSION, JobTemplate, NeedSource,
        PassedSecrets, RunDefaults, Service, StepTemplate, Strategy,
    },
};

// Actions: reusable workflows.
/// How many levels of workflows may call each other, the run's own included (GitHub's limit).
pub const MAX_CALL_DEPTH: usize = 10;
/// How many different workflows one run may call (GitHub's limit).
pub const MAX_CALLED_WORKFLOWS: usize = 50;
/// The longest job key the database keeps.
pub const MAX_JOB_KEY_BYTES: usize = 100;

/// Finds the workflows that jobs call.
pub trait Loader {
    /// The workflow `target` names, called from the workflow that [`Loaded::origin`] named
    /// (`None`: the run's own). The error says why it cannot be used, without telling a
    /// private repository from a missing one.
    fn load(&mut self, origin: Option<&str>, target: &CallTarget) -> Result<Loaded, String>;
}

/// A called workflow.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub workflow: Workflow,
    /// Where its own `./` calls are found: opaque to planning, given back to [`Loader::load`].
    pub origin: String,
    /// The file at its commit, to count different workflows.
    pub id: String,
}

/// The [`Loader`] of a workflow that may call none: every call is refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCalls;

impl Loader for NoCalls {
    fn load(&mut self, _origin: Option<&str>, _target: &CallTarget) -> Result<Loaded, String> {
        Err("calling a workflow needs the rbit server".to_owned())
    }
}

/// One matrix combination: each key and its value, keys in sorted order.
pub type Combination = Map<String, Value>;

/// What the server knows of a run when it plans it: the contexts of §12.4's first row
/// besides `matrix` and `strategy`, which planning works out itself. The field types are
/// those of [`crate::spec::Contexts`], so the same values go to the runner later.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunContext {
    /// The `github` context, `event` included and the token never.
    pub github: Map<String, Value>,
    /// `inputs` of a `workflow_dispatch` run; empty otherwise.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables; empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
}

impl RunContext {
    /// `github`, `inputs` and `vars`, for [`crate::expr`].
    pub fn contexts(&self) -> ContextMap {
        let vars: Map<String, Value> = self.vars.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
        ContextMap::new().with("github", self.github.clone()).with("inputs", self.inputs.clone()).with("vars", vars)
    }
}

/// The run's contexts plus one job's `matrix` and `strategy`, without copying the former.
struct JobContexts<'a> {
    run: &'a ContextMap,
    matrix: Value,
    strategy: Value,
}

impl Contexts for JobContexts<'_> {
    fn context(&self, name: &str) -> Option<&Value> {
        match name {
            "matrix" => Some(&self.matrix),
            "strategy" => Some(&self.strategy),
            _ => self.run.context(name),
        }
    }
}

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
    let scope = CallScope { prefix: String::new(), origin: None, depth: 1, calls: Vec::new(), cap: None, root_needs: Vec::new() };
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
                Some(matrix) => combinations(&Axes::resolve(matrix, run, &job.id)?, self.limits, self.deadline)?,
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
            let inner =
                CallScope { prefix, origin: Some(loaded.origin.clone()), depth: scope.depth + 1, calls, cap, root_needs: needs.clone() };
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

/// The secrets a call passes, checked against those the called workflow declares.
fn passed_secrets(
    evaluator: &Evaluator<'_>,
    secrets: &CallSecrets,
    declared: &crate::model::WorkflowCall,
) -> Result<PassedSecrets, WorkflowError> {
    match secrets {
        CallSecrets::Inherit => Ok(PassedSecrets::Inherit),
        CallSecrets::Map(map) => {
            if let Some(name) = map.keys().find(|name| !declared.secrets.contains_key(*name)) {
                return Err(evaluator.refuse(&format!("secrets.{name}"), &"the called workflow has no such secret"));
            }
            if let Some(name) = declared.secrets.iter().find_map(|(name, required)| (*required && !map.contains_key(name)).then_some(name))
            {
                return Err(evaluator.refuse("secrets", &format!("the called workflow needs the secret `{name}`")));
            }
            Ok(PassedSecrets::Map(map.clone()))
        }
    }
}

// Actions: plans from needs.
/// Whether what the server plans of `job` (`name`, `runs-on`, `strategy`, `timeout-minutes`,
/// `continue-on-error`, `concurrency`, `container`, `services`, `environment`) reads `needs`,
/// as GitHub allows: then it is planned when they are done.
fn reads_needs(job: &Job) -> bool {
    if job.needs.is_empty() {
        return false;
    }
    let mut texts: Vec<&str> = job.runs_on.iter().map(String::as_str).collect();
    texts.extend(job.name.as_deref());
    texts.extend(job.timeout_minutes.as_deref());
    texts.extend(job.continue_on_error.as_deref());
    texts.extend(job.concurrency.as_ref().map(|concurrency| concurrency.group.as_str()));
    let matrix = job.strategy.matrix.as_ref().filter(|matrix| matrix.has_expressions).map(|matrix| matrix.value.to_string());
    texts.extend(matrix.as_deref());
    texts.extend(job.strategy.fail_fast.as_deref());
    texts.extend(job.strategy.max_parallel.as_deref());
    for (image, options, volumes) in job
        .container
        .iter()
        .map(|container| (&container.image, &container.options, &container.volumes))
        .chain(job.services.iter().map(|service| (&service.image, &service.options, &service.volumes)))
    {
        texts.push(image);
        texts.extend(options.as_deref());
        texts.extend(volumes.iter().map(String::as_str));
    }
    if let Some(environment) = &job.environment {
        texts.push(&environment.name);
    }
    texts.iter().any(|text| text.contains("${{") && text.contains("needs"))
}

/// The blocked stand-in of a job planned once its needs are done ([`JobTemplate::deferred`]):
/// its `if:` (GitHub's default `success()` when it has none) is decided first.
fn deferred_template(job: &Job, key: &str, needs: &[String]) -> JobTemplate {
    JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: key.to_owned(),
        name: job.name.clone().filter(|name| !name.contains("${{")).unwrap_or_else(|| job.id.clone()),
        needs: needs.to_vec(),
        condition: Some(job.condition.clone().unwrap_or_else(|| "success()".to_owned())),
        runs_on: Vec::new(),
        matrix: Map::new(),
        strategy: Strategy { fail_fast: true, max_parallel: None, job_index: 0, job_total: 1 },
        permissions: None,
        concurrency: None,
        timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        continue_on_error: false,
        workflow_env: BTreeMap::new(),
        env: BTreeMap::new(),
        defaults: RunDefaults::default(),
        outputs: BTreeMap::new(),
        container: None,
        services: Vec::new(),
        steps: Vec::new(),
        environment: None,
        environment_url: None,
        calls: Vec::new(),
        need_names: BTreeMap::new(),
        deferred: true,
    }
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
    let scope = CallScope { prefix: String::new(), origin: None, depth: 1, calls: Vec::new(), cap: None, root_needs: Vec::new() };
    let need_names: BTreeMap<String, NeedSource> =
        job.needs.iter().map(|need| (need.clone(), NeedSource::Jobs(vec![need.clone()]))).collect();
    let combinations = match &job.strategy.matrix {
        Some(matrix) => combinations(&Axes::resolve(matrix, run, &job.id)?, limits, deadline)?,
        None => vec![Combination::new()],
    };
    let evaluator = Evaluator { job, contexts: run };
    let fail_fast = job.strategy.fail_fast.as_deref().map(|text| evaluator.flag("strategy.fail-fast", text)).transpose()?.unwrap_or(true);
    let max_parallel =
        job.strategy.max_parallel.as_deref().map(|text| evaluator.whole_number("strategy.max-parallel", text)).transpose()?;
    let job_total = u32::try_from(combinations.len()).unwrap_or(u32::MAX);
    let expansion = Expansion { combinations, fail_fast, max_parallel, job_total, need_names, needs: job.needs.clone() };
    let (mut templates, _) = planner.steps_job(workflow, job, run, &scope, expansion)?;
    // The decision was made for the stand-in: the planned jobs run.
    for template in &mut templates {
        template.condition = None;
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

/// The `strategy` context of `strategy`.
fn strategy_context(strategy: Strategy) -> Value {
    json!({
        "fail-fast": strategy.fail_fast,
        "job-index": strategy.job_index,
        "job-total": strategy.job_total,
        "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
    })
}

/// `declared` permissions within `cap`: what a job gets (the cap when it declares none). A
/// declaration past the cap fails planning, as on GitHub.
fn narrowed(
    declared: Option<Permissions>,
    cap: Option<Permissions>,
    job: &str,
    at: Location,
) -> Result<Option<Permissions>, WorkflowError> {
    let (Some(declared), Some(cap)) = (declared, cap) else { return Ok(declared.or(cap)) };
    let rank = |permission: Permission| match permission {
        Permission::None => 0,
        Permission::Read => 1,
        Permission::Write => 2,
    };
    let name = |permission: Permission| match permission {
        Permission::None => "none",
        Permission::Read => "read",
        Permission::Write => "write",
    };
    for (field, asked, allowed) in [
        ("contents", declared.contents, cap.contents),
        ("issues", declared.issues, cap.issues),
        ("pull-requests", declared.pull_requests, cap.pull_requests),
    ] {
        if rank(asked) > rank(allowed) {
            return Err(WorkflowError::plan(
                format!(
                    "job `{job}` asks for `{field}: {}`, but the workflow calling it allows only `{field}: {}`",
                    name(asked),
                    name(allowed)
                ),
                Some(at),
            ));
        }
    }
    Ok(Some(declared))
}

/// The called workflow's inputs: typed values known now, and the `with:` that read `needs`.
/// The `inputs` context of a called workflow, and the inputs only its jobs' runner can
/// evaluate.
type CallInputs = (Map<String, Value>, BTreeMap<String, DeferredInput>);

fn call_inputs(
    evaluator: &Evaluator<'_>,
    with: &BTreeMap<String, String>,
    declared: &crate::model::WorkflowCall,
) -> Result<CallInputs, WorkflowError> {
    if let Some(name) = with.keys().find(|name| !declared.inputs.contains_key(*name)) {
        return Err(evaluator.refuse(&format!("with.{name}"), &"the called workflow has no such input"));
    }
    let (mut inputs, mut deferred) = (Map::new(), BTreeMap::new());
    for (name, input) in &declared.inputs {
        let field = format!("with.{name}");
        match with.get(name) {
            Some(text) => {
                let template = Template::parse(text).map_err(|problem| evaluator.refuse(&field, &problem))?;
                if template.exprs().any(|expr| expr.contexts().contains(&"needs")) {
                    deferred.insert(name.clone(), DeferredInput { value: text.clone(), kind: input.kind.as_str().to_owned() });
                } else {
                    let value = evaluator.text(&field, text)?;
                    let typed = call_input_value(input.kind, &value)
                        .ok_or_else(|| evaluator.refuse(&field, &format!("`{value}` is not a {}", input.kind.as_str())))?;
                    inputs.insert(name.clone(), typed);
                }
            }
            None => match (&input.default, input.required) {
                (Some(default), _) => {
                    inputs.insert(name.clone(), default.clone());
                }
                (None, true) => return Err(evaluator.refuse("with", &format!("the called workflow needs the input `{name}`"))),
                (None, false) => {
                    let empty = match input.kind {
                        CallInputKind::Boolean => Value::Bool(false),
                        CallInputKind::Number => json!(0),
                        CallInputKind::String => Value::String(String::new()),
                    };
                    inputs.insert(name.clone(), empty);
                }
            },
        }
    }
    Ok((inputs, deferred))
}

/// One job of one combination as a template.
/// The job's `container` and `services`, evaluated.
fn containers(evaluator: &Evaluator<'_>, job: &Job) -> Result<(Option<Container>, Vec<Service>), WorkflowError> {
    let container = match &job.container {
        Some(container) => Some(Container {
            image: evaluator.image("container.image", &container.image)?,
            env: container.env.clone(),
            credentials: container.credentials.clone(),
            options: evaluator.options("container", container.options.as_deref(), false)?,
            volumes: evaluator.volumes("container", &container.volumes)?,
        }),
        None => None,
    };
    let services = job
        .services
        .iter()
        .map(|service| {
            Ok(Service {
                id: service.id.clone(),
                image: evaluator.image(&format!("services.{}.image", service.id), &service.image)?,
                env: service.env.clone(),
                credentials: service.credentials.clone(),
                options: evaluator.options(&format!("services.{}", service.id), service.options.as_deref(), true)?,
                volumes: evaluator.volumes(&format!("services.{}", service.id), &service.volumes)?,
            })
        })
        .collect::<Result<_, WorkflowError>>()?;
    Ok((container, services))
}

fn job_template(
    workflow: &Workflow,
    job: &Job,
    run: &ContextMap,
    combination: Combination,
    strategy: Strategy,
    limits: &Limits,
) -> Result<JobTemplate, WorkflowError> {
    let contexts = JobContexts {
        run,
        matrix: Value::Object(combination.clone()),
        strategy: json!({
            "fail-fast": strategy.fail_fast,
            "job-index": strategy.job_index,
            "job-total": strategy.job_total,
            "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
        }),
    };
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
    let timeout = match &job.timeout_minutes {
        Some(text) => evaluator.minutes("timeout-minutes", text)?,
        None => DEFAULT_TIMEOUT_MINUTES,
    };
    let concurrency = match &job.concurrency {
        Some(concurrency) => {
            let group = evaluator.text("concurrency.group", &concurrency.group)?;
            if group.is_empty() {
                return Err(evaluator.refuse("concurrency.group", &"evaluates to an empty group"));
            }
            Some(ConcurrencyGroup { group, cancel_in_progress: concurrency.cancel_in_progress })
        }
        None => None,
    };
    let (container, services) = containers(&evaluator, job)?;
    // Actions: environments.
    let environment = match &job.environment {
        Some(environment) => {
            let name = evaluator.text("environment", &environment.name)?;
            let name = name.trim();
            if !crate::spec::valid_environment_name(name) {
                return Err(evaluator.refuse("environment", &format!("`{}` is not an environment name", name.escape_debug())));
            }
            Some(name.to_owned())
        }
        None => None,
    };
    let template = JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: job.id.clone(),
        name,
        needs: job.needs.clone(),
        condition: job.condition.clone(),
        runs_on: evaluator.runs_on()?,
        matrix: combination,
        strategy,
        permissions: job.permissions.or(workflow.permissions).map(Into::into),
        concurrency,
        timeout_minutes: timeout.min(limits.max_timeout_minutes.max(1)),
        continue_on_error: job
            .continue_on_error
            .as_deref()
            .map(|text| evaluator.flag("continue-on-error", text))
            .transpose()?
            .unwrap_or(false),
        workflow_env: workflow.env.clone(),
        env: job.env.clone(),
        defaults: RunDefaults {
            shell: job.defaults.shell.clone().or_else(|| workflow.defaults.shell.clone()),
            working_directory: job.defaults.working_directory.clone().or_else(|| workflow.defaults.working_directory.clone()),
        },
        outputs: job.outputs.clone(),
        container,
        services,
        steps: job.steps.iter().map(step_template).collect(),
        environment,
        environment_url: job.environment.as_ref().and_then(|environment| environment.url.clone()),
        calls: Vec::new(),
        need_names: std::collections::BTreeMap::new(),
        deferred: false,
    };
    template.validate().map_err(|problem| WorkflowError::plan(format!("job `{}`: {}", job.id, problem.message), Some(job.at)))?;
    Ok(template)
}

pub(crate) fn step_template(step: &Step) -> StepTemplate {
    StepTemplate {
        id: step.id.clone(),
        name: step.name.clone(),
        condition: step.condition.clone(),
        action: step.action.clone(),
        shell: step.shell.clone(),
        working_directory: step.working_directory.clone(),
        env: step.env.clone(),
        continue_on_error: step.continue_on_error.clone(),
        timeout_minutes: step.timeout_minutes.clone(),
    }
}

/// Evaluates one job's server-side fields, with errors that name the job and the field.
struct Evaluator<'a> {
    job: &'a Job,
    contexts: &'a dyn Contexts,
}

impl Evaluator<'_> {
    fn refuse(&self, field: &str, problem: &dyn std::fmt::Display) -> WorkflowError {
        WorkflowError::plan(format!("job `{}`: `{field}`: {problem}", self.job.id), Some(self.job.at))
    }

    fn value(&self, field: &str, text: &str) -> Result<Value, WorkflowError> {
        Template::parse(text).and_then(|template| template.evaluate(self.contexts)).map_err(|problem| self.refuse(field, &problem))
    }

    fn text(&self, field: &str, text: &str) -> Result<String, WorkflowError> {
        Template::parse(text).and_then(|template| template.render(self.contexts)).map_err(|problem| self.refuse(field, &problem))
    }

    /// `options` of `field` (`container`, `services.<id>`), evaluated and checked.
    fn options(&self, field: &str, text: Option<&str>, service: bool) -> Result<Vec<String>, WorkflowError> {
        let Some(text) = text else { return Ok(Vec::new()) };
        let field = format!("{field}.options");
        let text = self.text(&field, text)?;
        crate::options::docker_options(&text, service).map_err(|problem| self.refuse(&field, &problem))
    }

    /// `volumes` of `field`, evaluated and checked.
    fn volumes(&self, field: &str, volumes: &[String]) -> Result<Vec<String>, WorkflowError> {
        let field = format!("{field}.volumes");
        volumes
            .iter()
            .map(|volume| {
                let volume = self.text(&field, volume)?;
                crate::options::volume(&volume).map_err(|problem| self.refuse(&field, &problem))?;
                Ok(volume.trim().to_owned())
            })
            .collect()
    }

    fn image(&self, field: &str, text: &str) -> Result<String, WorkflowError> {
        let image = self.text(field, text)?;
        if image.trim().is_empty() || image.chars().any(char::is_whitespace) {
            return Err(self.refuse(field, &format!("`{image}` is not an image name")));
        }
        Ok(image)
    }

    /// `true`, `false`, or an expression giving one of them.
    fn flag(&self, field: &str, text: &str) -> Result<bool, WorkflowError> {
        match self.value(field, text)? {
            Value::Bool(value) => Ok(value),
            Value::String(value) if value.eq_ignore_ascii_case("true") => Ok(true),
            Value::String(value) if value.eq_ignore_ascii_case("false") => Ok(false),
            other => Err(self.refuse(field, &format!("must be true or false, not `{}`", to_text(&other)))),
        }
    }

    /// A positive number of minutes, rounded up.
    fn minutes(&self, field: &str, text: &str) -> Result<u32, WorkflowError> {
        let value = self.value(field, text)?;
        let minutes = to_number(&value);
        if !(minutes > 0.0 && minutes.is_finite()) {
            return Err(self.refuse(field, &format!("must be a positive number, not `{}`", to_text(&value))));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "positive, and saturated at u32::MAX")]
        Ok(minutes.ceil().min(f64::from(u32::MAX)) as u32)
    }

    /// A whole number of at least 1.
    fn whole_number(&self, field: &str, text: &str) -> Result<u32, WorkflowError> {
        let value = self.value(field, text)?;
        let number = to_number(&value);
        if !(number >= 1.0 && number.fract() == 0.0 && number <= f64::from(u32::MAX)) {
            return Err(self.refuse(field, &format!("must be a whole number of at least 1, not `{}`", to_text(&value))));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "checked to be a whole number in range")]
        Ok(number as u32)
    }

    /// `runs-on`, evaluated. A label that is exactly one expression whose value is a list
    /// (`${{ matrix.labels }}`) contributes every item.
    fn runs_on(&self) -> Result<Vec<String>, WorkflowError> {
        let mut labels = Vec::new();
        for label in &self.job.runs_on {
            match self.value("runs-on", label)? {
                Value::Array(items) => {
                    for item in items {
                        let Value::String(item) = item else {
                            return Err(self.refuse("runs-on", &format!("`{label}` must give strings")));
                        };
                        labels.push(item);
                    }
                }
                Value::Object(_) => return Err(self.refuse("runs-on", &format!("`{label}` must give a string or a list of strings"))),
                other => labels.push(to_text(&other)),
            }
        }
        if labels.is_empty() || labels.iter().any(String::is_empty) {
            return Err(self.refuse("runs-on", &"evaluates to an empty label"));
        }
        Ok(labels)
    }
}

/// A matrix with its expressions evaluated and its shape checked, ready to expand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Axes {
    /// Each dimension and its values.
    pub dimensions: Vec<(String, Vec<Value>)>,
    pub include: Vec<Combination>,
    pub exclude: Vec<Combination>,
    /// Where the matrix is, for errors.
    pub at: Option<Location>,
}

impl Axes {
    /// Evaluates the expressions of `matrix` with the run's contexts and checks the result.
    pub fn resolve(matrix: &Matrix, contexts: &dyn Contexts, job_id: &str) -> Result<Self, WorkflowError> {
        let refuse =
            |problem: &dyn std::fmt::Display| WorkflowError::plan(format!("job `{job_id}`: `strategy.matrix`: {problem}"), Some(matrix.at));
        let value = if matrix.has_expressions {
            evaluate_strings(&matrix.value, contexts).map_err(|problem| refuse(&problem))?
        } else {
            matrix.value.clone()
        };
        let mut axes = Self::from_json(&value).map_err(|problem| refuse(&problem))?;
        axes.at = Some(matrix.at);
        Ok(axes)
    }

    /// The matrix in `value`: an object of lists, with `include` and `exclude` lists of
    /// objects whose `exclude` keys are dimensions, and a dimension or an `include`.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let Value::Object(entries) = value else {
            return Err("must be a mapping of lists".to_owned());
        };
        let mut axes = Self::default();
        for (key, value) in entries {
            let Value::Array(items) = value else {
                return Err(format!("`{key}` must be a list"));
            };
            match key.as_str() {
                "include" | "exclude" => {
                    let entries = items
                        .iter()
                        .map(|item| item.as_object().cloned().ok_or_else(|| format!("`{key}` must be a list of mappings")))
                        .collect::<Result<Vec<_>, _>>()?;
                    if key == "include" {
                        axes.include = entries;
                    } else {
                        axes.exclude = entries;
                    }
                }
                _ => axes.dimensions.push((key.clone(), items.clone())),
            }
        }
        for entry in &axes.exclude {
            if let Some(key) = entry.keys().find(|key| !axes.dimensions.iter().any(|(name, _)| name == *key)) {
                return Err(format!("`exclude`: `{key}` is not a matrix dimension"));
            }
        }
        if axes.dimensions.is_empty() && axes.include.is_empty() {
            return Err("must have a dimension or an `include`".to_owned());
        }
        Ok(axes)
    }
}

/// `value` with every string holding `${{ }}` replaced by its value.
fn evaluate_strings(value: &Value, contexts: &dyn Contexts) -> Result<Value, crate::expr::ExprError> {
    Ok(match value {
        Value::String(text) if text.contains("${{") => Template::parse(text)?.evaluate(contexts)?,
        Value::Array(items) => Value::Array(items.iter().map(|item| evaluate_strings(item, contexts)).collect::<Result<_, _>>()?),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), evaluate_strings(value, contexts)?)))
                .collect::<Result<_, crate::expr::ExprError>>()?,
        ),
        other => other.clone(),
    })
}

/// Expands a matrix within `limits` (see the module documentation for the rules).
pub fn combinations(axes: &Axes, limits: &Limits, deadline: &Deadline) -> Result<Vec<Combination>, WorkflowError> {
    let too_many = || WorkflowError::limit(format!("a matrix may have at most {} combinations", limits.max_combinations), axes.at);
    let product = axes.dimensions.iter().try_fold(1usize, |count, (_, values)| count.checked_mul(values.len()));
    if product.is_none_or(|count| count > limits.max_combinations) {
        return Err(too_many());
    }
    let mut combinations: Vec<Combination> = if axes.dimensions.is_empty() { Vec::new() } else { vec![Combination::new()] };
    for (key, values) in &axes.dimensions {
        deadline.check()?;
        combinations = combinations
            .into_iter()
            .flat_map(|combination| {
                values.iter().map(move |value| {
                    let mut next = combination.clone();
                    next.insert(key.clone(), value.clone());
                    next
                })
            })
            .collect();
    }
    combinations.retain(|combination| {
        !axes.exclude.iter().any(|exclude| combination.iter().all(|(key, value)| exclude.get(key).is_none_or(|excluded| excluded == value)))
    });
    let original = |key: &str| axes.dimensions.iter().any(|(name, _)| name == key);
    let mut extra = Vec::new();
    for include in &axes.include {
        deadline.check()?;
        let mut matched = false;
        for combination in &mut combinations {
            let fits = combination.iter().all(|(key, value)| !original(key) || include.get(key).is_none_or(|included| included == value));
            if fits {
                matched = true;
                combination.extend(include.iter().map(|(key, value)| (key.clone(), value.clone())));
            }
        }
        if !matched {
            extra.push(include.clone());
        }
    }
    combinations.extend(extra);
    if combinations.len() > limits.max_combinations {
        return Err(too_many());
    }
    if combinations.is_empty() {
        combinations.push(Combination::new());
    }
    combinations.sort_by_cached_key(matrix_name);
    Ok(combinations)
}

/// `(value, value)`, values in key order, formatted as Go's `fmt.Sprint` formats them.
fn matrix_name(combination: &Combination) -> String {
    let values: Vec<String> = combination.values().map(go_format).collect();
    format!("({})", values.join(", "))
}

/// A value as Go's `fmt.Sprint` prints it once decoded into `interface{}`.
fn go_format(value: &Value) -> String {
    match value {
        Value::Null => "<nil>".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(integer), _) => integer.to_string(),
            (_, Some(integer)) => integer.to_string(),
            _ => go_float(number.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(text) => text.clone(),
        Value::Array(items) => format!("[{}]", items.iter().map(go_format).collect::<Vec<_>>().join(" ")),
        Value::Object(entries) => {
            format!("map[{}]", entries.iter().map(|(key, value)| format!("{key}:{}", go_format(value))).collect::<Vec<_>>().join(" "))
        }
    }
}

/// Go's `%v` for a float64: the shortest representation, with an exponent (`1e+21`,
/// `1e-05`) below 1e-4 and from 1e21.
fn go_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf".to_owned() } else { "-Inf".to_owned() };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-4..1e21).contains(&magnitude) {
        let formatted = format!("{value:e}");
        let (mantissa, exponent) = formatted.split_once('e').unwrap_or((&formatted, "0"));
        let (sign, digits) = exponent.strip_prefix('-').map_or(("+", exponent), |digits| ("-", digits));
        return format!("{mantissa}e{sign}{digits:0>2}");
    }
    format!("{value}")
}

/// The workflow's `run-name` with the run's contexts, or `None` when it has none or it
/// evaluates to nothing.
pub fn run_name(workflow: &Workflow, context: &RunContext) -> Result<Option<String>, WorkflowError> {
    let Some(run_name) = &workflow.run_name else {
        return Ok(None);
    };
    let name = Template::parse(run_name)
        .and_then(|template| template.render(&context.contexts()))
        .map_err(|problem| WorkflowError::plan(format!("run-name: {problem}"), None))?;
    Ok(Some(name.trim().to_owned()).filter(|name| !name.is_empty()))
}

/// What the server knows of a run before it exists, as the triggers fill it in.
/// [`GroupContext::run_context`] turns it into the [`RunContext`] a run is planned with: the
/// `github` context a task gets later (§12.4), less what only the run has. `run_id` is a
/// placeholder (planning tells a group naming it apart by planning twice), `run_number` and
/// `run_attempt` are absent, and `github.event` carries the pull request number, the
/// dispatch inputs or the schedule — the task's context has the whole event payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupContext {
    /// The workflow's name, or its path when it has none.
    pub workflow: String,
    /// The workflow file's path (`.rbit/workflows/ci.yml`), for `github.workflow_ref`.
    pub workflow_path: String,
    /// The full ref (`refs/heads/main`, `refs/pull/7/head`).
    pub reference: String,
    /// Pull requests only: the head branch.
    pub head_ref: String,
    /// Pull requests only: the base branch.
    pub base_ref: String,
    pub event_name: String,
    /// `owner/name`.
    pub repository: String,
    pub run_id: i64,
    pub pull_request_number: Option<u64>,
    /// The commit the run is for.
    pub sha: String,
    /// Logins of the account that caused the run and of the one that triggered it; empty
    /// when nobody did (schedules).
    pub actor: String,
    pub triggering_actor: String,
    pub repository_owner: String,
    pub repository_id: i64,
    /// The instance's public URL, without a trailing slash.
    pub server_url: String,
    /// `schedule` runs only: the cron expression that fired.
    pub schedule: String,
    /// `workflow_dispatch` only: the checked inputs, typed.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables; empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
}

impl GroupContext {
    /// The values as a `github` context. Ids are strings, as the task's context has them.
    pub fn github(&self) -> Map<String, Value> {
        let reference = self.reference.as_str();
        let ref_name =
            ["refs/heads/", "refs/tags/", "refs/pull/"].iter().find_map(|prefix| reference.strip_prefix(prefix)).unwrap_or(reference);
        let mut event = Map::new();
        if let Some(number) = self.pull_request_number {
            event.insert("number".to_owned(), json!(number));
            event.insert("pull_request".to_owned(), json!({ "number": number }));
        }
        if !self.inputs.is_empty() {
            let as_text = self.inputs.iter().map(|(name, value)| (name.clone(), Value::String(crate::expr::to_text(value)))).collect();
            event.insert("inputs".to_owned(), Value::Object(as_text));
        }
        if !self.schedule.is_empty() {
            event.insert("schedule".to_owned(), json!(self.schedule));
        }
        let github = json!({
            "workflow": self.workflow,
            "workflow_ref": format!("{}/{}@{}", self.repository, self.workflow_path, self.reference),
            "ref": self.reference,
            "ref_name": ref_name,
            "ref_type": if reference.starts_with("refs/tags/") { "tag" } else { "branch" },
            "head_ref": self.head_ref,
            "base_ref": self.base_ref,
            "sha": self.sha,
            "event_name": self.event_name,
            "event": event,
            "repository": self.repository,
            "repository_owner": self.repository_owner,
            "repository_id": self.repository_id.to_string(),
            "actor": self.actor,
            "triggering_actor": self.triggering_actor,
            "run_id": self.run_id.to_string(),
            "server_url": self.server_url,
            "api_url": format!("{}/api/v1", self.server_url),
        });
        github.as_object().cloned().unwrap_or_default()
    }

    /// The [`RunContext`] a run is planned with: [`Self::github`], the inputs and variables.
    pub fn run_context(&self) -> RunContext {
        RunContext { github: self.github(), inputs: self.inputs.clone(), vars: self.vars.clone() }
    }
}

/// The run's concurrency group and whether it cancels runs in progress, or `None` when
/// the workflow sets no concurrency.
pub fn concurrency_group(workflow: &Workflow, context: &RunContext) -> Result<Option<(String, bool)>, WorkflowError> {
    let Some(concurrency) = &workflow.concurrency else {
        return Ok(None);
    };
    let refuse = |problem: &dyn std::fmt::Display| WorkflowError::plan(format!("concurrency group: {problem}"), Some(concurrency.at));
    let group = Template::parse(&concurrency.group)
        .and_then(|template| template.render(&context.contexts()))
        .map_err(|problem| refuse(&problem))?;
    if group.is_empty() {
        return Err(refuse(&"evaluates to an empty group"));
    }
    Ok(Some((group, concurrency.cancel_in_progress)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, spec::BuiltinKind, spec::StepAction};

    fn workflow(source: &str) -> Workflow {
        parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
    }

    /// A push to `main` of `alice/app`, dispatched with `target=arm64`, with `RUNNERS` set.
    fn run_context() -> RunContext {
        RunContext {
            github: json!({"ref": "refs/heads/main", "repository": "alice/app", "event_name": "push", "workflow": "CI", "run_id": 7})
                .as_object()
                .cloned()
                .unwrap(),
            inputs: json!({"target": "arm64", "minutes": 45, "matrix": "{\"os\": [\"x\", \"y\"]}"}).as_object().cloned().unwrap(),
            vars: BTreeMap::from([("RUNNER".into(), "big".into()), ("OSES".into(), "[\"debian\", \"alpine\"]".into())]),
        }
    }

    fn planned(source: &str) -> Vec<JobTemplate> {
        plan(&workflow(source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
    }

    fn matrix_of(matrix: &str) -> Vec<String> {
        let source = format!("on: push\njobs:\n  j:\n    steps: [{{run: x}}]\n    runs-on: x\n    strategy:\n      matrix:\n{matrix}\n");
        let workflow = workflow(&source);
        let axes = Axes::resolve(workflow.jobs[0].strategy.matrix.as_ref().unwrap(), &run_context().contexts(), "j").unwrap();
        combinations(&axes, &Limits::default(), &Deadline::NONE)
            .unwrap()
            .iter()
            .map(|combination| combination.iter().map(|(key, value)| format!("{key}={}", go_format(value))).collect::<Vec<_>>().join(","))
            .collect()
    }

    #[test]
    fn matrix_expansion_table() {
        let cases: &[(&str, &[&str])] = &[
            ("        os: [a, b]", &["os=a", "os=b"]),
            ("        os: [b, a]\n        v: [2, 1]", &["os=a,v=1", "os=a,v=2", "os=b,v=1", "os=b,v=2"]),
            ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a, v: 2}", &["os=a,v=1", "os=b,v=1", "os=b,v=2"]),
            ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a}", &["os=b,v=1", "os=b,v=2"]),
            // An include that fits merges into every combination it fits.
            ("        os: [a, b]\n        include:\n          - {os: a, extra: x}", &["os=b", "extra=x,os=a"]),
            ("        os: [a, b]\n        include:\n          - {extra: x}", &["extra=x,os=a", "extra=x,os=b"]),
            // An include that contradicts every original value stands alone.
            ("        os: [a, b]\n        include:\n          - {os: c, extra: x}", &["os=a", "os=b", "extra=x,os=c"]),
            // Added values may be overwritten by a later include; original ones never.
            ("        os: [a]\n        include:\n          - {extra: x}\n          - {extra: y}", &["extra=y,os=a"]),
            // GitHub's documented example.
            (
                "        fruit: [apple, pear]\n        animal: [cat, dog]\n        include:\n          - color: green\n          - color: pink\n            animal: cat\n          - fruit: apple\n            shape: circle\n          - fruit: banana\n          - fruit: banana\n            animal: cat",
                &[
                    "fruit=banana",
                    "animal=cat,fruit=banana",
                    "animal=cat,color=pink,fruit=apple,shape=circle",
                    "animal=cat,color=pink,fruit=pear",
                    "animal=dog,color=green,fruit=apple,shape=circle",
                    "animal=dog,color=green,fruit=pear",
                ],
            ),
            // Only includes: each is a combination.
            ("        include:\n          - {os: a}\n          - {os: b, v: 1}", &["os=a", "os=b,v=1"]),
            // An empty dimension leaves nothing but the includes, or one empty combination.
            ("        os: []", &[""]),
            ("        os: []\n        include:\n          - {os: z}", &["os=z"]),
            // Values compare as data: 1.10 equals 1.1, 1 does not equal '1'.
            ("        v: [1.10, 2]\n        exclude:\n          - {v: 1.1}", &["v=2"]),
            ("        v: [1, 2]\n        exclude:\n          - {v: '1'}", &["v=1", "v=2"]),
            ("        cfg: [{a: 1}, [x, y]]", &["cfg=[x y]", "cfg=map[a:1]"]),
            // Expressions over the run's contexts, anywhere in the matrix.
            ("        os: ${{ fromJSON(vars.OSES) }}", &["os=alpine", "os=debian"]),
            ("        os: ['${{ inputs.target }}', '${{ vars.RUNNER }}-1']", &["os=arm64", "os=big-1"]),
            ("        os: [a]\n        include: '${{ fromJSON(''[{\"os\": \"b\"}]'') }}'", &["os=a", "os=b"]),
        ];
        for (matrix, expected) in cases {
            assert_eq!(matrix_of(matrix), *expected, "{matrix}");
        }
        let whole =
            "on: push\njobs:\n  j:\n    runs-on: x\n    strategy: {matrix: '${{ fromJSON(inputs.matrix) }}'}\n    steps: [{run: x}]\n";
        let names: Vec<_> = planned(whole).into_iter().map(|job| job.name).collect();
        assert_eq!(names, ["j (x)", "j (y)"]);
    }

    #[test]
    fn evaluated_matrices_are_checked() {
        let cases = [
            ("'${{ inputs.target }}'", "must be a mapping of lists"),
            ("{os: '${{ inputs.target }}'}", "`os` must be a list"),
            ("{os: [a], include: '${{ fromJSON(''[1]'') }}'}", "`include` must be a list of mappings"),
            ("{os: [a], exclude: '${{ fromJSON(''[{\"v\": 1}]'') }}'}", "`v` is not a matrix dimension"),
            ("{os: '${{ fromJSON(''[]'') }}', include: []}", ""),
            ("'${{ fromJSON(inputs.target) }}'", "fromJSON"),
        ];
        for (matrix, message) in cases {
            let source = format!("on: push\njobs:\n  j:\n    runs-on: x\n    strategy: {{matrix: {matrix}}}\n    steps: [{{run: x}}]\n");
            let result = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE);
            if message.is_empty() {
                assert_eq!(result.unwrap().len(), 1, "{matrix}");
                continue;
            }
            let error = result.unwrap_err();
            assert_eq!(error.kind, crate::ErrorKind::Plan, "{matrix}");
            assert!(error.message.contains(message) && error.message.starts_with("job `j`: `strategy.matrix`"), "{matrix}: {error}");
        }
    }

    #[test]
    fn matrix_limits() {
        let limits = Limits { max_combinations: 4, ..Limits::default() };
        let axes = |source: &str| {
            Axes::resolve(workflow(source).jobs[0].strategy.matrix.as_ref().unwrap(), &run_context().contexts(), "j").unwrap()
        };
        let source = "on: push\njobs:\n  j:\n    steps: [{run: x}]\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3]\n        b: [1, 2]\n";
        let error = combinations(&axes(source), &limits, &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Limit);
        let source = "on: push\njobs:\n  j:\n    steps: [{run: x}]\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3, 4]\n        include: [{a: 5}]\n";
        assert!(combinations(&axes(source), &limits, &Deadline::NONE).is_err(), "includes count too");
        let run_limits = Limits { max_run_jobs: 5, ..Limits::default() };
        let source = "on: push\njobs:\n  a:\n    steps: [{run: x}]\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n  b:\n    steps: [{run: x}]\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n";
        let error = plan(&workflow(source), &run_context(), &run_limits, &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Limit);
        let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
        assert_eq!(plan(&workflow(source), &run_context(), &Limits::default(), &expired).unwrap_err().kind, crate::ErrorKind::Timeout);
    }

    #[test]
    fn go_formatting() {
        let cases = [(1.17, "1.17"), (3.0, "3"), (1e21, "1e+21"), (1.5e-5, "1.5e-05"), (0.0001, "0.0001"), (-2.5, "-2.5"), (0.0, "0")];
        for (value, expected) in cases {
            assert_eq!(go_float(value), expected, "{value}");
        }
        assert_eq!(go_float(f64::NAN), "NaN");
        assert_eq!(go_format(&json!({"b": null, "a": [1, 2.5, true]})), "map[a:[1 2.5 true] b:<nil>]");
    }

    #[test]
    fn names_and_labels() {
        let source = "on: push
jobs:
  plain:
    runs-on: linux
    steps: [{run: x}]
  named:
    name: Build ${{ matrix.os }} for ${{ inputs.target }} (${{ strategy.job-index }}/${{ strategy.job-total }})
    runs-on: [self-hosted, '${{ matrix.os }}-${{ matrix.arch || ''x64'' }}']
    steps: [{run: x}]
    strategy:
      matrix:
        os: [debian]
  listed:
    runs-on: ${{ matrix.labels }}
    steps: [{run: x}]
    strategy:
      matrix:
        labels: [[self-hosted, arm64]]
        n: [1.10]
  github:
    name: ${{ github.repository }}
    runs-on: ['${{ vars.RUNNER }}', '${{ github.ref == ''refs/heads/main'' && ''release'' || ''dev'' }}']
    steps: [{run: x}]
  empty-name:
    name: ${{ inputs.missing }}
    runs-on: x
    steps: [{run: x}]
";
        let jobs = planned(source);
        let summary: Vec<(&str, &str, Vec<&str>)> =
            jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str(), job.runs_on.iter().map(String::as_str).collect())).collect();
        assert_eq!(
            summary,
            [
                ("plain", "plain", vec!["linux"]),
                ("named", "Build debian for arm64 (0/1)", vec!["self-hosted", "debian-x64"]),
                ("listed", "listed ([self-hosted arm64], 1.1)", vec!["self-hosted", "arm64"]),
                ("github", "alice/app", vec!["big", "release"]),
                ("empty-name", "empty-name", vec!["x"]),
            ]
        );
        assert!(jobs[0].matrix.is_empty());
        assert_eq!(serde_json::to_string(&jobs[2].matrix).unwrap(), r#"{"labels":["self-hosted","arm64"],"n":1.1}"#);
    }

    #[test]
    fn runs_on_refusals() {
        for runs_on in ["${{ matrix.missing }}", "${{ matrix.empty }}", "${{ matrix.nested }}", "${{ fromJSON('{}') }}", "two words"] {
            let source = format!(
                "on: push\njobs:\n  j:\n    steps: [{{run: x}}]\n    runs-on: \"{runs_on}\"\n    strategy: {{matrix: {{empty: [''], nested: [[{{a: 1}}]]}}}}\n"
            );
            let error = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_err();
            assert_eq!(error.kind, crate::ErrorKind::Plan, "{runs_on}");
            assert!(error.message.starts_with("job `j`"), "{error}");
        }
    }

    // Actions: environments.
    #[test]
    fn environments_are_named_when_planned_and_their_url_on_the_runner() {
        let jobs = planned(
            "on: push
jobs:
  deploy:
    runs-on: x
    strategy: {matrix: {stage: [staging, production]}}
    environment:
      name: ${{ matrix.stage }}
      url: ${{ steps.deploy.outputs.url }}
    steps: [{run: x}]
  plain:
    runs-on: x
    environment: Production
    steps: [{run: x}]
",
        );
        let environments: Vec<_> = jobs.iter().map(|job| (job.environment.as_deref(), job.environment_url.as_deref())).collect();
        assert_eq!(
            environments,
            [
                (Some("production"), Some("${{ steps.deploy.outputs.url }}")),
                (Some("staging"), Some("${{ steps.deploy.outputs.url }}")),
                (Some("Production"), None)
            ]
        );
        for name in ["${{ inputs.missing }}", "a/b"] {
            let source = format!(
                "on: push
jobs:
  j:
    runs-on: x
    environment: '{name}'
    steps: [{{run: x}}]
"
            );
            let error = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_err();
            assert!(error.message.contains("environment"), "{name}: {error}");
        }
    }

    const EVERY_FIELD: &str = "\
name: CI
on: push
env: {CARGO_TERM_COLOR: always}
defaults:
  run: {shell: bash, working-directory: crates}
permissions: read-all
jobs:
  build:
    runs-on: linux
    steps: [{run: cargo build}]
  test:
    name: Test ${{ matrix.os }}
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ${{ matrix.os }}
    permissions: {contents: write}
    env: {RUST: '${{ matrix.rust }}'}
    defaults:
      run: {shell: sh}
    outputs:
      version: ${{ steps.meta.outputs.version }}
    timeout-minutes: ${{ inputs.minutes }}
    continue-on-error: ${{ matrix.os == 'windows' }}
    concurrency:
      group: test-${{ github.ref }}-${{ matrix.os }}
      cancel-in-progress: true
    strategy:
      fail-fast: false
      max-parallel: ${{ fromJSON('1') }}
      matrix:
        os: [linux, windows]
    container:
      image: rust:${{ matrix.os }}
      credentials: {username: ci, password: '${{ secrets.REGISTRY }}'}
    services:
      db:
        image: postgres:${{ strategy.job-total }}
        env: {POSTGRES_PASSWORD: '${{ secrets.DB }}'}
    steps:
      - uses: actions/checkout@v4
        with: {fetch-depth: 0}
      - id: meta
        name: Meta
        if: always()
        run: echo version=1 >> \"$GITHUB_OUTPUT\"
        shell: bash
        working-directory: x
        env: {A: b}
        continue-on-error: true
        timeout-minutes: 5
";

    #[test]
    fn every_field_is_filled() {
        let jobs = planned(EVERY_FIELD);
        assert_eq!(jobs.len(), 3);
        let build = &jobs[0];
        assert_eq!((build.timeout_minutes, build.continue_on_error, build.strategy), (DEFAULT_TIMEOUT_MINUTES, false, Strategy::SINGLE));
        assert_eq!(build.permissions, Some(crate::model::Permissions::uniform(crate::model::Permission::Read).into()));
        assert_eq!(build.defaults, RunDefaults { shell: Some("bash".into()), working_directory: Some("crates".into()) });
        let (linux, windows) = (&jobs[1], &jobs[2]);
        assert_eq!((linux.name.as_str(), windows.name.as_str()), ("Test linux", "Test windows"));
        assert_eq!(linux.version, JOB_TEMPLATE_VERSION);
        assert_eq!(linux.needs, ["build"]);
        assert_eq!(linux.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
        assert_eq!((linux.runs_on.as_slice(), windows.runs_on.as_slice()), (&["linux".to_owned()][..], &["windows".to_owned()][..]));
        assert_eq!(serde_json::Value::Object(linux.matrix.clone()), json!({"os": "linux"}));
        assert_eq!(linux.strategy, Strategy { fail_fast: false, max_parallel: Some(1), job_index: 0, job_total: 2 });
        assert_eq!(windows.strategy.job_index, 1);
        assert_eq!(
            serde_json::to_value(linux.permissions).unwrap(),
            json!({"contents": "write", "issues": "none", "pull_requests": "none"})
        );
        assert_eq!(linux.concurrency, Some(ConcurrencyGroup { group: "test-refs/heads/main-linux".into(), cancel_in_progress: true }));
        assert_eq!(linux.timeout_minutes, 45);
        assert_eq!((linux.continue_on_error, windows.continue_on_error), (false, true));
        assert_eq!(linux.workflow_env, crate::spec::Env::from([("CARGO_TERM_COLOR".into(), "always".into())]));
        assert_eq!(linux.env, crate::spec::Env::from([("RUST".into(), "${{ matrix.rust }}".into())]));
        assert_eq!(linux.defaults, RunDefaults { shell: Some("sh".into()), working_directory: Some("crates".into()) });
        assert_eq!(linux.outputs, BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]));
        let container = linux.container.as_ref().unwrap();
        assert_eq!(container.image, "rust:linux");
        assert_eq!(container.credentials.as_ref().map(|credentials| credentials.password.as_str()), Some("${{ secrets.REGISTRY }}"));
        assert_eq!(linux.services.len(), 1);
        assert_eq!((linux.services[0].id.as_str(), linux.services[0].image.as_str()), ("db", "postgres:2"));
        assert_eq!(linux.services[0].env, crate::spec::Env::from([("POSTGRES_PASSWORD".into(), "${{ secrets.DB }}".into())]));
        assert!(matches!(&linux.steps[0].action, StepAction::Uses(builtin) if builtin.kind == BuiltinKind::Checkout));
        let meta = &linux.steps[1];
        assert_eq!(
            meta,
            &StepTemplate {
                id: Some("meta".into()),
                name: Some("Meta".into()),
                condition: Some("always()".into()),
                action: StepAction::Run("echo version=1 >> \"$GITHUB_OUTPUT\"".into()),
                shell: Some("bash".into()),
                working_directory: Some("x".into()),
                env: crate::spec::Env::from([("A".into(), "b".into())]),
                continue_on_error: Some("true".into()),
                timeout_minutes: Some("5".into()),
            }
        );
        for job in &jobs {
            job.validate().unwrap();
        }
    }

    #[test]
    fn timeouts_flags_and_numbers() {
        let job = |settings: &str| {
            let source = format!("on: push\njobs:\n  j:\n    runs-on: x\n{settings}    steps: [{{run: x}}]\n");
            plan(&workflow(&source), &run_context(), &Limits { max_timeout_minutes: 120, ..Limits::default() }, &Deadline::NONE)
        };
        let timeout = |settings: &str| job(settings).map(|jobs| jobs[0].timeout_minutes);
        assert_eq!(timeout(""), Ok(120), "the default, capped");
        assert_eq!(timeout("    timeout-minutes: 30\n"), Ok(30));
        assert_eq!(timeout("    timeout-minutes: 2.5\n"), Ok(3), "rounded up");
        assert_eq!(timeout("    timeout-minutes: 600\n"), Ok(120), "capped");
        assert_eq!(timeout("    timeout-minutes: ${{ inputs.minutes }}\n"), Ok(45));
        assert_eq!(timeout("    timeout-minutes: \"${{ format('{0}', 7) }}\"\n"), Ok(7));
        assert_eq!(job("    timeout-minutes: ${{ inputs.target }}\n").unwrap_err().kind, crate::ErrorKind::Plan);
        assert_eq!(job("    timeout-minutes: ${{ inputs.missing }}\n").unwrap_err().kind, crate::ErrorKind::Plan);
        let default = plan(
            &workflow("on: push\njobs:\n  j:\n    runs-on: x\n    steps: [{run: x}]\n"),
            &run_context(),
            &Limits::default(),
            &Deadline::NONE,
        );
        assert_eq!(default.unwrap()[0].timeout_minutes, DEFAULT_TIMEOUT_MINUTES);
        let flag = |settings: &str| job(settings).map(|jobs| (jobs[0].continue_on_error, jobs[0].strategy.fail_fast));
        assert_eq!(flag(""), Ok((false, true)));
        assert_eq!(flag("    continue-on-error: true\n    strategy: {fail-fast: false}\n"), Ok((true, false)));
        assert_eq!(flag("    continue-on-error: ${{ inputs.target == 'arm64' }}\n"), Ok((true, true)));
        assert_eq!(flag("    continue-on-error: \"${{ 'TRUE' }}\"\n"), Ok((true, true)));
        assert!(job("    continue-on-error: ${{ inputs.target }}\n").unwrap_err().message.contains("must be true or false, not `arm64`"));
        let parallel = |settings: &str| job(settings).map(|jobs| jobs[0].strategy.max_parallel);
        assert_eq!(parallel(""), Ok(None));
        assert_eq!(parallel("    strategy: {max-parallel: 3}\n"), Ok(Some(3)));
        assert!(job("    strategy: {max-parallel: '${{ fromJSON(''2.5'') }}'}\n").unwrap_err().message.contains("whole number"));
        assert!(job("    concurrency: '${{ inputs.missing }}'\n").unwrap_err().message.contains("empty group"));
        assert!(job("    container: '${{ inputs.missing }}'\n").unwrap_err().message.contains("is not an image name"));
    }

    #[test]
    fn run_names() {
        let named = |text: &str| {
            run_name(&workflow(&format!("on: push\nrun-name: {text}\njobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n")), &run_context())
        };
        assert_eq!(
            named("'Deploy ${{ inputs.target }} from ${{ github.ref }}'").unwrap().as_deref(),
            Some("Deploy arm64 from refs/heads/main")
        );
        assert_eq!(named("'${{ inputs.missing }}'").unwrap(), None);
        assert_eq!(run_name(&workflow("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n"), &run_context()).unwrap(), None);
    }

    #[test]
    fn concurrency_groups() {
        let context = GroupContext {
            workflow: "CI".into(),
            workflow_path: ".rbit/workflows/ci.yml".into(),
            reference: "refs/pull/7/head".into(),
            head_ref: "feature".into(),
            base_ref: "main".into(),
            event_name: "pull_request".into(),
            repository: "alice/rocket".into(),
            run_id: 31,
            pull_request_number: Some(7),
            sha: "0123456789abcdef0123456789abcdef01234567".into(),
            actor: "bob".into(),
            triggering_actor: "bob".into(),
            repository_owner: "alice".into(),
            repository_id: 4,
            server_url: "https://git.example.com".into(),
            ..GroupContext::default()
        };
        let group = |concurrency: &str| {
            let source = format!("on: push\nconcurrency: {concurrency}\njobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n");
            concurrency_group(&workflow(&source), &context.run_context())
        };
        assert_eq!(group("ci").unwrap(), Some(("ci".into(), false)));
        assert_eq!(group("'${{ github.workflow }}-${{ github.ref }}'").unwrap(), Some(("CI-refs/pull/7/head".into(), false)));
        assert_eq!(
            group("{group: '${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}', cancel-in-progress: true}")
                .unwrap(),
            Some(("CI-7".into(), true))
        );
        assert_eq!(group("'${{ github.head_ref || github.run_id }}'").unwrap(), Some(("feature".into(), false)));
        assert_eq!(
            group("'${{ github.repository }}/${{ github.event_name }}/${{ github.base_ref }}'").unwrap().unwrap().0,
            "alice/rocket/pull_request/main"
        );
        assert_eq!(group("'${{ github.actor }}-${{ github.sha }}'").unwrap().unwrap().0, "bob-0123456789abcdef0123456789abcdef01234567");
        assert_eq!(group("'${{ github.ref_name }}@${{ github.server_url }}'").unwrap().unwrap().0, "7/head@https://git.example.com");
        assert!(group("'${{ github.run_number }}'").is_err(), "unknown before the run exists, so the group is empty");
        assert!(group("'${{ github.base_ref && null }}'").is_err(), "an empty group is refused");
        let push = GroupContext { pull_request_number: None, head_ref: String::new(), ..context };
        let source = "on: push\nconcurrency: '${{ github.event.pull_request.number || github.run_id }}'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n";
        assert_eq!(concurrency_group(&workflow(source), &push.run_context()).unwrap(), Some(("31".into(), false)));
        assert_eq!(
            concurrency_group(&workflow("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n"), &push.run_context()).unwrap(),
            None
        );
        let source = "on: push\nconcurrency: '${{ vars.RUNNER }}-${{ inputs.target }}'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n";
        assert_eq!(concurrency_group(&workflow(source), &run_context()).unwrap(), Some(("big-arm64".into(), false)));
    }

    /// Each case plans a workflow and compares its job templates, as pretty JSON, with the
    /// expected file. The inputs are Gitea's job parser test data (MIT, see
    /// `THIRD-PARTY.md`), the outputs rbit's.
    const GOLDEN: [(&str, &str, &str); 7] = [
        ("multiple_jobs", include_str!("../tests/golden/multiple_jobs.in.yaml"), include_str!("../tests/golden/multiple_jobs.out.json")),
        (
            "multiple_matrix",
            include_str!("../tests/golden/multiple_matrix.in.yaml"),
            include_str!("../tests/golden/multiple_matrix.out.json"),
        ),
        (
            "job_name_with_matrix",
            include_str!("../tests/golden/job_name_with_matrix.in.yaml"),
            include_str!("../tests/golden/job_name_with_matrix.out.json"),
        ),
        ("has_needs", include_str!("../tests/golden/has_needs.in.yaml"), include_str!("../tests/golden/has_needs.out.json")),
        (
            "prefixed_newline",
            include_str!("../tests/golden/prefixed_newline.in.yaml"),
            include_str!("../tests/golden/prefixed_newline.out.json"),
        ),
        (
            "continue_on_error_expr",
            include_str!("../tests/golden/continue_on_error_expr.in.yaml"),
            include_str!("../tests/golden/continue_on_error_expr.out.json"),
        ),
        (
            "include_exclude",
            include_str!("../tests/golden/include_exclude.in.yaml"),
            include_str!("../tests/golden/include_exclude.out.json"),
        ),
    ];

    #[test]
    fn golden_templates() {
        // RBIT_BLESS=1 rewrites the expected files after a deliberate change.
        let bless = std::env::var_os("RBIT_BLESS").is_some();
        for (name, input, expected) in GOLDEN {
            let jobs = planned(input);
            let actual = serde_json::to_string_pretty(&jobs).unwrap() + "\n";
            if bless {
                std::fs::write(format!("{}/tests/golden/{name}.out.json", env!("CARGO_MANIFEST_DIR")), &actual).unwrap();
                continue;
            }
            assert_eq!(actual, expected.replace("\r\n", "\n"), "{name}");
            assert_eq!(serde_json::from_str::<Vec<JobTemplate>>(expected).unwrap(), jobs, "{name}: the templates read back");
        }
    }

    // Actions: reusable workflows.
    /// Workflows by path, all in one repository; a call outside it is not there.
    struct Files(BTreeMap<&'static str, &'static str>);

    impl Loader for Files {
        fn load(&mut self, origin: Option<&str>, target: &CallTarget) -> Result<Loaded, String> {
            let path = match target {
                CallTarget::Local { path } => path.clone(),
                CallTarget::Remote { owner, repo, path, reference } => format!("{owner}/{repo}/{path}@{reference}"),
            };
            let source = self.0.get(path.as_str()).ok_or_else(|| format!("`{path}` is not there"))?;
            Ok(Loaded { workflow: workflow(source), origin: origin.unwrap_or("here").to_owned(), id: path })
        }
    }

    fn planned_with(source: &str, files: &[(&'static str, &'static str)]) -> Result<Vec<JobTemplate>, WorkflowError> {
        plan_with(&workflow(source), &run_context(), &Limits::default(), &Deadline::NONE, &mut Files(files.iter().copied().collect()))
    }

    const BUILD: &str = "on:
  workflow_call:
    inputs:
      greeting: {type: string, required: true}
      count: {type: number, default: 2}
      debug: {type: boolean}
      late: {type: string}
    secrets:
      key: {required: true}
    outputs:
      result: {value: '${{ jobs.build.outputs.out }}'}
permissions:
  contents: read
jobs:
  build:
    runs-on: ${{ inputs.greeting }}-runner
    outputs: {out: '${{ steps.s.outputs.out }}'}
    steps: [{id: s, run: x}]
  test:
    needs: build
    runs-on: x
    steps: [{run: x}]
";

    #[test]
    fn called_workflows_become_jobs_of_the_run() {
        let caller = "on: push
jobs:
  first:
    runs-on: x
    steps: [{run: x}]
  call:
    needs: first
    if: github.ref == 'refs/heads/main'
    uses: ./.rbit/workflows/build.yml
    with: {greeting: hello, late: '${{ needs.first.outputs.x }}'}
    secrets: {key: '${{ secrets.DEPLOY }}'}
  after:
    needs: call
    runs-on: x
    steps: [{run: x}]
";
        let jobs = planned_with(caller, &[(".rbit/workflows/build.yml", BUILD)]).unwrap();
        let keys: Vec<_> = jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str())).collect();
        assert_eq!(keys, [("first", "first"), ("call/build", "call / build"), ("call/test", "call / test"), ("after", "after")]);
        let (build, test, after) = (&jobs[1], &jobs[2], &jobs[3]);
        assert_eq!(build.runs_on, ["hello-runner"], "runs-on reads the inputs known when planning");
        assert_eq!(build.needs, ["first"], "the called workflow waits for what its caller waits for");
        assert_eq!(test.needs, ["call/build"]);
        assert_eq!(test.need_names, BTreeMap::from([("build".to_owned(), NeedSource::Jobs(vec!["call/build".to_owned()]))]));
        assert!(build.need_names.is_empty(), "a called workflow's first job sees no `needs`");
        let level = &build.calls[0];
        assert_eq!(level.inputs, json!({"greeting": "hello", "count": 2.0, "debug": false}).as_object().cloned().unwrap());
        assert_eq!(level.deferred["late"], DeferredInput { value: "${{ needs.first.outputs.x }}".into(), kind: "string".into() });
        assert_eq!(level.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
        assert_eq!(level.secrets, PassedSecrets::Map(BTreeMap::from([("key".into(), "${{ secrets.DEPLOY }}".into())])));
        assert_eq!(build.permissions.map(|permissions| permissions.contents), Some(crate::spec::Access::Read));
        assert_eq!(after.needs, ["call/build", "call/test"]);
        let NeedSource::Call { keys, outputs, jobs } = &after.need_names["call"] else { panic!("{:?}", after.need_names) };
        assert_eq!((keys.len(), outputs["result"].as_str()), (2, "${{ jobs.build.outputs.out }}"));
        assert_eq!(jobs["build"], NeedSource::Jobs(vec!["call/build".into()]));
    }

    #[test]
    fn matrices_and_nesting_call_once_per_combination() {
        let caller = "on: push
jobs:
  call:
    strategy: {matrix: {flavor: [a, b]}}
    uses: ./outer.yml
    with: {flavor: '${{ matrix.flavor }}'}
    secrets: inherit
";
        let outer = "on: {workflow_call: {inputs: {flavor: {type: string}}}}
jobs:
  inner:
    uses: ./leaf.yml
    with: {flavor: '${{ inputs.flavor }}'}
";
        let leaf = "on: {workflow_call: {inputs: {flavor: {type: string}}}}
jobs:
  leaf:
    runs-on: ${{ inputs.flavor }}
    steps: [{run: x}]
";
        let jobs = planned_with(caller, &[("outer.yml", outer), ("leaf.yml", leaf)]).unwrap();
        let keys: Vec<_> = jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str(), job.runs_on[0].as_str())).collect();
        assert_eq!(keys, [("call#0/inner/leaf", "call (a) / inner / leaf", "a"), ("call#1/inner/leaf", "call (b) / inner / leaf", "b")]);
        assert_eq!(jobs[0].calls.len(), 2);
        assert_eq!(jobs[0].calls[0].secrets, PassedSecrets::Inherit);
    }

    #[test]
    fn calls_are_checked_when_the_run_is_planned() {
        let call = |with: &str, secrets: &str| {
            format!(
                "on: push\npermissions: {{contents: read}}\njobs:\n  call:\n    uses: ./b.yml\n    with: {with}\n    secrets: {secrets}\n"
            )
        };
        let files = [("b.yml", BUILD), ("plain.yml", "on: push\njobs:\n  a: {runs-on: x, steps: [{run: x}]}\n")];
        for (source, message) in [
            (call("{}", "{key: x}"), "needs the input `greeting`"),
            (call("{greeting: hi, nope: 1}", "{key: x}"), "`with.nope`: the called workflow has no such input"),
            (call("{greeting: hi, count: many}", "{key: x}"), "`many` is not a number"),
            (call("{greeting: hi}", "{}"), "needs the secret `key`"),
            (call("{greeting: hi}", "{key: x, other: y}"), "`secrets.other`: the called workflow has no such secret"),
            ("on: push\njobs:\n  call: {uses: ./plain.yml}\n".to_owned(), "does not run on `workflow_call`"),
            ("on: push\njobs:\n  call: {uses: ./missing.yml}\n".to_owned(), "`missing.yml` is not there"),
        ] {
            let error = planned_with(&source, &files).unwrap_err().to_string();
            assert!(error.contains(message), "{source}\n{error}");
        }
        let writer = "on: workflow_call\njobs:\n  w:\n    runs-on: x\n    permissions: {contents: write}\n    steps: [{run: x}]\n";
        let caller = "on: push\npermissions: {contents: read}\njobs:\n  call:\n    uses: ./w.yml\n";
        let error = planned_with(caller, &[("w.yml", writer)]).unwrap_err().to_string();
        assert!(error.contains("asks for `contents: write`, but the workflow calling it allows only `contents: read`"), "{error}");
        assert!(
            plan(&workflow("on: push\njobs:\n  call: {uses: ./b.yml}\n"), &run_context(), &Limits::default(), &Deadline::NONE).is_err()
        );
    }

    #[test]
    fn calls_nest_at_most_ten_levels_deep() {
        let forever = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
        let leaf = "on: workflow_call\njobs:\n  leaf: {runs-on: x, steps: [{run: x}]}\n";
        let deep = |called: usize| {
            let mut files: Vec<(&'static str, &'static str)> = (1..called)
                .map(|n| (forever(format!("w{n}.yml")), forever(format!("on: workflow_call\njobs:\n  d:\n    uses: ./w{}.yml\n", n + 1))))
                .collect();
            files.push((forever(format!("w{called}.yml")), leaf));
            planned_with("on: push\njobs:\n  d:\n    uses: ./w1.yml\n", &files)
        };
        assert!(deep(9).is_ok(), "the run's workflow and nine called ones: ten levels");
        let error = deep(10).unwrap_err().to_string();
        assert!(error.contains("more than 10 levels deep"), "{error}");
    }
}
