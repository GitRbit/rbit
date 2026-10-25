//! Running one job: the [`Executor`] of `rbit-runner run` (§12.5.4 "Execution").
//!
//! A job is prepared once — its label picks the backend, its directories are created
//! under the task directory (`workspace`, `temp`, and `files` for the env files and
//! `event.json`), the task environment is set and the workflow's and job's `env` are
//! evaluated — and then runs its steps in order:
//!
//! * a step's `if` (default `success()`) sees the job's status so far through the status
//!   functions; a false condition skips the step;
//! * the step's `env`, `with`, `working-directory`, `shell`, `continue-on-error` and its
//!   script are evaluated when it starts, with the `env`, `steps`, `job` and `runner`
//!   contexts as they are at that moment;
//! * a failed step fails the job unless its `continue-on-error` holds; its outcome and
//!   conclusion go into the `steps` context either way;
//! * a cancelled task cancels the running step; later steps only run if their condition
//!   asks for it (`always()`, `cancelled()`).
//!
//! After the last step the job's `outputs` are evaluated, masked and reported.

use std::{collections::BTreeMap, future::Future, sync::Arc};

use rbit_workflow::{
    expr::{Condition, ContextMap, Status, Template, Value, to_text},
    protocol::{Backend as BackendKind, MAX_OUTPUT_BYTES, StepStatus, TaskResult},
    spec::{Env, JobSpec, StepAction, StepTemplate},
};
use serde_json::{Map, json};

use crate::{
    backend::{self, Backend, Dirs, View, host::Host},
    config::Loaded,
    contexts::{self, Eval},
    steps::{self, Outcome},
    task::{Executor, Reporter},
};

/// The executor of `rbit-runner run`.
#[derive(Debug, Clone)]
pub struct JobExecutor {
    loaded: Arc<Loaded>,
}

impl JobExecutor {
    pub fn new(loaded: Arc<Loaded>) -> Self {
        Self { loaded }
    }
}

impl Executor for JobExecutor {
    fn execute(&self, spec: JobSpec, reporter: Arc<Reporter>) -> impl Future<Output = TaskResult> + Send {
        let loaded = Arc::clone(&self.loaded);
        async move {
            let steps = spec.template.steps.len();
            match Job::prepare(&loaded, spec, Arc::clone(&reporter)) {
                Ok(mut job) => job.run().await,
                Err(message) => {
                    reporter.log(&format!("##[error]{message}"));
                    for index in 0..steps {
                        reporter.finish_step(index, StepStatus::Skipped);
                    }
                    TaskResult::Failure
                }
            }
        }
    }
}

/// How the job stands, as the status functions see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Success,
    Failure,
    Cancelled,
}

impl Phase {
    const fn status(self) -> Status {
        match self {
            Self::Success => Status::Success,
            Self::Failure => Status::Failure,
            Self::Cancelled => Status::Cancelled,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
        }
    }
}

/// A job being run.
#[derive(Debug)]
pub struct Job {
    pub spec: JobSpec,
    pub reporter: Arc<Reporter>,
    pub backend: Backend,
    pub dirs: Dirs,
    /// Every context, `env`, `steps` and `job` kept current.
    pub contexts: ContextMap,
    /// The variables of §12.5.4 every step gets.
    pub task_env: BTreeMap<String, String>,
    /// The `env` context of the job: workflow and job `env`, then `GITHUB_ENV` entries.
    pub env: BTreeMap<String, String>,
    /// `GITHUB_PATH` entries, newest first.
    pub path: Vec<String>,
    /// The `steps` context.
    pub steps: Map<String, Value>,
    pub phase: Phase,
}

impl Job {
    /// Picks the backend, creates the directories and evaluates the job's environment.
    pub fn prepare(loaded: &Loaded, spec: JobSpec, reporter: Arc<Reporter>) -> Result<Self, String> {
        let template = &spec.template;
        let label = loaded
            .config
            .label_for(&template.runs_on)
            .ok_or_else(|| format!("this runner serves none of the labels `{}`", template.runs_on.join("`, `")))?
            .clone();
        let dirs = Dirs::create(reporter.dir()).map_err(|error| format!("cannot create the task directories: {error}"))?;
        let backend = match label.backend {
            BackendKind::Host => {
                if template.container.is_some() {
                    return Err(format!(
                        "the job runs in a `container`, but label `{}` runs jobs on the host; use a container label",
                        label.name
                    ));
                }
                if !template.services.is_empty() {
                    return Err(format!("`services` need a container label; label `{}` runs jobs on the host", label.name));
                }
                Backend::Host(Host::new(&dirs))
            }
            BackendKind::Container => return Err("the container backend is not available in this build of rbit-runner yet".to_owned()),
        };
        reporter.log(&format!("Runner `{}`, label `{}` ({} backend)", loaded.config.name, label.name, backend_name(label.backend)));

        let view = backend.view().clone();
        let event_path = dirs.files.join("event.json");
        let event = spec.contexts.github.get("event").cloned().unwrap_or_else(|| json!({}));
        std::fs::write(&event_path, serde_json::to_vec_pretty(&event).unwrap_or_default())
            .map_err(|error| format!("cannot write event.json: {error}"))?;
        let event_view = join(&view.files, "event.json", backend.windows());

        let mut github = spec.contexts.github.clone();
        github.insert("workspace".into(), Value::String(view.workspace.clone()));
        github.insert("event_path".into(), Value::String(event_view));
        github.insert("token".into(), Value::String(spec.token.expose().to_owned()));
        github.entry("job").or_insert_with(|| Value::String(template.job_id.clone()));
        github.entry("action").or_insert_with(|| Value::String(String::new()));
        let (os, arch) = (backend.os(), backend::runner_arch(std::env::consts::ARCH));
        let runner = json!({
            "name": loaded.config.name,
            "os": os,
            "arch": arch,
            "temp": view.temp,
            "environment": "self-hosted",
        });
        let task_env = task_env(&github, &view, &loaded.config.name, os, arch);
        let contexts = contexts::job_contexts(&spec, github, runner);

        let mut job = Self {
            spec,
            reporter,
            backend,
            dirs,
            contexts,
            task_env,
            env: BTreeMap::new(),
            path: Vec::new(),
            steps: Map::new(),
            phase: Phase::Success,
        };
        job.sync_contexts();
        let (workflow_env, job_env) = (job.spec.template.workflow_env.clone(), job.spec.template.env.clone());
        for env in [workflow_env, job_env] {
            let evaluated = job.render_env(&env).map_err(|error| format!("`env`: {error}"))?;
            job.env.extend(evaluated);
            job.sync_contexts();
        }
        Ok(job)
    }

    /// Runs every step, then evaluates the job's outputs.
    pub async fn run(&mut self) -> TaskResult {
        let steps = self.spec.template.steps.clone();
        for (index, step) in steps.iter().enumerate() {
            if self.reporter.cancellation().is_cancelled() && self.phase != Phase::Cancelled {
                self.phase = Phase::Cancelled;
                self.sync_contexts();
            }
            let run = match self.condition(step) {
                Ok(run) => run,
                Err(error) => {
                    self.reporter.start_step(index);
                    self.reporter.log(&format!("##[error]`if`: {error}"));
                    self.reporter.finish_step(index, StepStatus::Failure);
                    self.record(step, &Outcome::failure(), StepStatus::Failure);
                    continue;
                }
            };
            if !run {
                self.reporter.finish_step(index, StepStatus::Skipped);
                self.record(step, &Outcome { status: StepStatus::Skipped, outputs: BTreeMap::new() }, StepStatus::Skipped);
                continue;
            }
            self.reporter.start_step(index);
            let outcome = self.run_step(index, step).await;
            let conclusion = match outcome.status {
                StepStatus::Failure if self.continue_on_error(step) => {
                    self.reporter.log("##[warning]The step failed; `continue-on-error` lets the job go on");
                    StepStatus::Success
                }
                status => status,
            };
            self.reporter.finish_step(index, outcome.status);
            self.record(step, &outcome, conclusion);
        }
        self.outputs();
        if self.reporter.cancellation().is_cancelled() {
            self.phase = Phase::Cancelled;
        }
        match self.phase {
            Phase::Success => TaskResult::Success,
            Phase::Failure => TaskResult::Failure,
            Phase::Cancelled => TaskResult::Cancelled,
        }
    }

    async fn run_step(&mut self, index: usize, step: &StepTemplate) -> Outcome {
        let step_env = match self.render_env(&step.env) {
            Ok(env) => env,
            Err(error) => return self.fail(&format!("`env`: {error}")),
        };
        self.set_env_context(&step_env);
        let outcome = match &step.action {
            StepAction::Run(script) => steps::run::run(self, index, step, script, &step_env).await,
            StepAction::Uses(builtin) => steps::builtin(self, builtin),
        };
        self.set_env_context(&BTreeMap::new());
        outcome
    }

    /// Logs `message` as an error and fails the step.
    pub fn fail(&self, message: &str) -> Outcome {
        self.reporter.log(&format!("##[error]{message}"));
        Outcome::failure()
    }

    fn condition(&self, step: &StepTemplate) -> Result<bool, String> {
        let condition = Condition::parse(step.condition.as_deref().unwrap_or_default()).map_err(|error| error.to_string())?;
        condition.evaluate(&self.eval(Some(self.phase.status()))).map_err(|error| error.to_string())
    }

    fn continue_on_error(&self, step: &StepTemplate) -> bool {
        match step.continue_on_error.as_deref() {
            None => false,
            Some(text) => match self.evaluate(text) {
                Ok(value) => rbit_workflow::expr::truthy(&value) && value != Value::String("false".into()),
                Err(error) => {
                    self.reporter.log(&format!("##[warning]`continue-on-error`: {error}"));
                    false
                }
            },
        }
    }

    /// Records a finished step in the `steps` context and the job's status.
    fn record(&mut self, step: &StepTemplate, outcome: &Outcome, conclusion: StepStatus) {
        match conclusion {
            StepStatus::Failure => self.phase = self.phase.max_with(Phase::Failure),
            StepStatus::Cancelled => self.phase = Phase::Cancelled,
            _ => {}
        }
        if let Some(id) = &step.id {
            let entry = json!({
                "outputs": outcome.outputs,
                "outcome": status_name(outcome.status),
                "conclusion": status_name(conclusion),
            });
            self.steps.insert(id.clone(), entry);
        }
        self.sync_contexts();
    }

    /// The contexts for an evaluation; `status` for conditions.
    pub fn eval(&self, status: Option<Status>) -> Eval<'_> {
        Eval { contexts: &self.contexts, status, workspace: Some(&self.dirs.workspace) }
    }

    /// `template` interpolated into a string.
    pub fn render(&self, template: &str) -> Result<String, String> {
        Template::parse(template).and_then(|parsed| parsed.render(&self.eval(None))).map_err(|error| error.to_string())
    }

    /// `template`'s value: the expression's own when it is exactly one `${{ }}`.
    pub fn evaluate(&self, template: &str) -> Result<Value, String> {
        Template::parse(template).and_then(|parsed| parsed.evaluate(&self.eval(None))).map_err(|error| error.to_string())
    }

    fn render_env(&self, env: &Env) -> Result<BTreeMap<String, String>, String> {
        env.iter().map(|(name, value)| Ok((name.clone(), self.render(value).map_err(|error| format!("{name}: {error}"))?))).collect()
    }

    /// The `env` context while a step runs: the job's, then the step's own.
    fn set_env_context(&mut self, step_env: &BTreeMap<String, String>) {
        let mut env = self.env.clone();
        env.extend(step_env.iter().map(|(name, value)| (name.clone(), value.clone())));
        self.contexts.insert("env", json!(env));
    }

    /// Writes `env`, `steps` and `job` into the contexts.
    pub fn sync_contexts(&mut self) {
        self.contexts.insert("env", json!(self.env));
        self.contexts.insert("steps", Value::Object(self.steps.clone()));
        self.contexts.insert("job", json!({"status": self.phase.name()}));
    }

    /// The environment of a step's process: the task's variables, the job's `env`, the
    /// step's `env`, `PATH` with the `GITHUB_PATH` entries in front, and `extra`.
    pub fn process_env(&self, step_env: &BTreeMap<String, String>, extra: Vec<(String, String)>) -> BTreeMap<String, String> {
        let mut env = self.task_env.clone();
        env.extend(self.env.iter().map(|(name, value)| (name.clone(), value.clone())));
        env.extend(step_env.iter().map(|(name, value)| (name.clone(), value.clone())));
        env.insert("PATH".to_owned(), self.search_path());
        env.extend(extra);
        env
    }

    /// `PATH` as steps see it.
    pub fn search_path(&self) -> String {
        let separator = self.backend.path_separator().to_string();
        let base = self.backend.base_path();
        self.path.iter().map(String::as_str).chain((!base.is_empty()).then_some(base.as_str())).collect::<Vec<_>>().join(&separator)
    }

    /// Evaluates and reports the job's `outputs`, masked and within the size cap.
    fn outputs(&mut self) {
        let mut outputs = BTreeMap::new();
        let mut total = 0usize;
        for (name, template) in self.spec.template.outputs.clone() {
            match self.render(&template) {
                Ok(value) => {
                    let value = self.reporter.mask(&value);
                    total += name.len() + value.len();
                    if total > MAX_OUTPUT_BYTES {
                        self.reporter.log(&format!(
                            "##[warning]job outputs exceed {MAX_OUTPUT_BYTES} bytes; `{name}` and later outputs are dropped"
                        ));
                        break;
                    }
                    outputs.insert(name, value);
                }
                Err(error) => self.reporter.log(&format!("##[warning]job output `{name}`: {error}")),
            }
        }
        self.reporter.set_outputs(outputs);
    }
}

impl Phase {
    /// The worse of two: a cancellation stays a cancellation.
    fn max_with(self, other: Self) -> Self {
        if self == Self::Cancelled { self } else { other }
    }
}

const fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Container => "container",
        BackendKind::Host => "host — no isolation",
    }
}

pub const fn status_name(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "success",
        StepStatus::Failure => "failure",
        StepStatus::Cancelled => "cancelled",
        StepStatus::Skipped | StepStatus::Waiting | StepStatus::Running => "skipped",
    }
}

/// `dir` and `name` joined with the backend's separator.
pub fn join(dir: &str, name: &str, windows: bool) -> String {
    let separator = if windows { '\\' } else { '/' };
    format!("{}{separator}{name}", dir.trim_end_matches(['/', '\\']))
}

/// The variables of §12.5.4: `CI`, `GITHUB_ACTIONS`, `RBIT_ACTIONS`, the `GITHUB_*` and
/// `RBIT_*` names of the run, and `RUNNER_*`. The token is not among them.
pub fn task_env(github: &Map<String, Value>, view: &View, runner: &str, os: &str, arch: &str) -> BTreeMap<String, String> {
    const FROM_GITHUB: [&str; 21] = [
        "repository",
        "repository_owner",
        "repository_id",
        "sha",
        "ref",
        "ref_name",
        "ref_type",
        "head_ref",
        "base_ref",
        "run_id",
        "run_number",
        "run_attempt",
        "job",
        "server_url",
        "api_url",
        "event_name",
        "actor",
        "triggering_actor",
        "workflow",
        "workspace",
        "event_path",
    ];
    let mut env = BTreeMap::from([
        ("CI".to_owned(), "true".to_owned()),
        ("GITHUB_ACTIONS".to_owned(), "true".to_owned()),
        ("RBIT_ACTIONS".to_owned(), "true".to_owned()),
        ("RUNNER_OS".to_owned(), os.to_owned()),
        ("RUNNER_ARCH".to_owned(), arch.to_owned()),
        ("RUNNER_TEMP".to_owned(), view.temp.clone()),
        ("RUNNER_NAME".to_owned(), runner.to_owned()),
    ]);
    for key in FROM_GITHUB {
        if let Some(value) = github.get(key).filter(|value| !value.is_null() && !value.is_object() && !value.is_array()) {
            let upper = key.to_ascii_uppercase();
            env.insert(format!("GITHUB_{upper}"), to_text(value));
            env.insert(format!("RBIT_{upper}"), to_text(value));
        }
    }
    env
}

#[cfg(test)]
mod tests;
