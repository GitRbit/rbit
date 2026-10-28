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

use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rbit_workflow::{
    expr::{Condition, ContextMap, Status, Template, Value, to_text},
    protocol::{Backend as BackendKind, MAX_OUTPUT_BYTES, StepStatus, TaskResult},
    spec::{Env, JobSpec, RegistryCredentials, StepAction, StepTemplate},
};
use serde_json::{Map, json};
use tokio_util::sync::CancellationToken;

use crate::{
    backend::{
        self, Backend, Dirs, View,
        container::{Container, Spec as ContainerSpec},
        host::Host,
    },
    client::Client,
    config::{ActionsConfig, ContainerConfig, Label, Loaded},
    contexts::{self, Eval},
    steps::{
        self, Outcome,
        cache::{PendingSave, Store},
    },
    task::{Executor, Reporter},
};

/// The executor of `rbit-runner run`.
#[derive(Debug, Clone)]
pub struct JobExecutor {
    loaded: Arc<Loaded>,
    /// The server the artifact built-ins talk to; `None` for `rbit-runner exec`.
    server: Option<Client>,
}

impl JobExecutor {
    pub fn new(loaded: Arc<Loaded>) -> Self {
        Self { loaded, server: None }
    }

    /// With the server's protocol client, for the artifact built-ins.
    #[must_use]
    pub fn with_server(mut self, client: Client) -> Self {
        self.server = Some(client);
        self
    }
}

impl Executor for JobExecutor {
    fn execute(&self, spec: JobSpec, reporter: Arc<Reporter>) -> impl Future<Output = TaskResult> + Send {
        let loaded = Arc::clone(&self.loaded);
        let server = self.server.clone();
        async move {
            let steps = spec.template.steps.len();
            let fail = |message: &str| {
                reporter.log(&format!("##[error]{message}"));
                for index in 0..steps {
                    reporter.finish_step(index, StepStatus::Skipped);
                }
                TaskResult::Failure
            };
            match Job::prepare(&loaded, spec, Arc::clone(&reporter)) {
                Ok(mut job) => {
                    job.server = server;
                    let result = match job.start().await {
                        Ok(()) => job.run().await,
                        Err(message) => fail(&message),
                    };
                    job.finish().await;
                    result
                }
                Err(message) => fail(&message),
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
    pub(crate) const fn status(self) -> Status {
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
    /// The label that chose the backend.
    pub label: Label,
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
    /// Fires when the task is cancelled, the job times out or the runner shuts down.
    pub interrupt: CancellationToken,
    /// Set when the job's own `timeout-minutes` fired the interrupt.
    pub timed_out: Arc<AtomicBool>,
    /// When the job first saw the interrupt; steps after it share the cancel budget.
    pub interrupted_at: Option<Instant>,
    /// The runner's cache.
    pub cache: Store,
    /// `rbit/cache` saves to make after the last step.
    pub post_saves: Vec<PendingSave>,
    /// The server, for the artifact built-ins; `None` without one (`rbit-runner exec`).
    pub server: Option<Client>,
    // Actions: third-party actions.
    /// The runner's work directory: the action cache, Node.js and the tool cache live there.
    pub work_dir: std::path::PathBuf,
    /// `[actions]`.
    pub actions: ActionsConfig,
    /// `[container]`, for Docker actions.
    pub container: ContainerConfig,
    /// The runner's name, which labels the containers of Docker actions.
    pub runner_name: String,
    /// Actions' `post` scripts, run after the last step in reverse order.
    pub post_steps: Vec<steps::action::PostStep>,
    /// `ACTIONS_STEP_DEBUG` is `true`: `::debug::` lines are printed.
    pub step_debug: bool,
}

/// How long steps that run after a cancellation (`always()`, `cancelled()`) may take
/// together.
pub const CANCEL_BUDGET: Duration = Duration::from_mins(5);

/// Why a step was stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    StepTimeout(Duration),
    Budget,
}

/// What stops one step, and afterwards why it did.
#[derive(Debug)]
pub struct StopSignal {
    interrupt: Option<CancellationToken>,
    deadline: Option<(Instant, StopReason)>,
    reason: Arc<std::sync::Mutex<Option<StopReason>>>,
}

impl StopSignal {
    /// Completes when the step must stop.
    pub fn wait(&self) -> impl Future<Output = ()> + Send + 'static {
        let (interrupt, deadline, reason) = (self.interrupt.clone(), self.deadline, Arc::clone(&self.reason));
        async move {
            let interrupted = async {
                match &interrupt {
                    Some(interrupt) => interrupt.cancelled().await,
                    None => std::future::pending().await,
                }
            };
            let expired = async {
                match deadline {
                    Some((at, _)) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            };
            let why = tokio::select! {
                () = interrupted => StopReason::Cancelled,
                () = expired => deadline.map_or(StopReason::Budget, |(_, why)| why),
            };
            *reason.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(why);
        }
    }

    pub fn reason(&self) -> StopReason {
        self.reason.lock().unwrap_or_else(std::sync::PoisonError::into_inner).unwrap_or(StopReason::Cancelled)
    }
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
        let interrupt = reporter.cancellation().child_token();
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
            BackendKind::Container => {
                let externals = loaded.work_dir.join("externals");
                let container = Container::new(&loaded.config, spec.task_id, &spec.checkout.repository, &dirs, reporter.dir(), &externals)?;
                Backend::Container(Box::new(container))
            }
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
        let mut task_env = task_env(&github, &view, &loaded.config.name, os, arch);
        let step_debug = action_env(&mut task_env, loaded, label.backend, &view, &spec)?;
        let contexts = contexts::job_contexts(&spec, github, runner);

        let mut job = Self {
            spec,
            reporter,
            backend,
            label,
            dirs,
            contexts,
            task_env,
            env: BTreeMap::new(),
            path: Vec::new(),
            steps: Map::new(),
            phase: Phase::Success,
            interrupt,
            timed_out: Arc::new(AtomicBool::new(false)),
            interrupted_at: None,
            cache: Store { root: loaded.work_dir.join("cache"), max_bytes: loaded.config.cache.max_bytes },
            post_saves: Vec::new(),
            server: None,
            work_dir: loaded.work_dir.clone(),
            actions: loaded.config.actions.clone(),
            container: loaded.config.container.clone(),
            runner_name: loaded.config.name.clone(),
            post_steps: Vec::new(),
            step_debug,
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

    /// The saves `rbit/cache` steps left, made only when the job succeeded.
    async fn save_caches(&mut self) {
        let saves = std::mem::take(&mut self.post_saves);
        if saves.is_empty() {
            return;
        }
        if self.phase != Phase::Success || self.interrupt.is_cancelled() {
            self.reporter.log("Not saving caches: the job did not succeed");
            return;
        }
        if let Err(error) = self.backend.pause(&self.reporter).await {
            self.reporter.log(&format!("##[warning]Not saving caches: cannot pause the job container: {error}"));
            return;
        }
        for save in saves {
            self.reporter.log(&format!("##[group]Post: save cache `{}`", save.key));
            if let Err(error) = steps::cache::save(self, &save.key, &save.paths, save.replace).await {
                self.reporter.log(&format!("##[warning]{error}"));
            }
            self.reporter.log("##[endgroup]");
        }
        let _ = self.backend.resume().await;
    }

    /// Starts what the job runs in: for the container backend, the network, the services
    /// and the job container, with their `env` and `credentials` evaluated now.
    pub async fn start(&mut self) -> Result<(), String> {
        if !matches!(self.backend, Backend::Container(_)) {
            return Ok(());
        }
        let template = self.spec.template.clone();
        let image = match &template.container {
            Some(container) => container.image.clone(),
            None => self.label.image.clone().unwrap_or_default(),
        };
        let job = ContainerSpec {
            image,
            env: match &template.container {
                Some(container) => self.render_env(&container.env).map_err(|error| format!("`container.env`: {error}"))?,
                None => BTreeMap::new(),
            },
            credentials: self.credentials(template.container.as_ref().and_then(|container| container.credentials.as_ref()), "container")?,
        };
        let mut services = Vec::new();
        for service in &template.services {
            let field = format!("services.{}", service.id);
            services.push((
                service.id.clone(),
                ContainerSpec {
                    image: service.image.clone(),
                    env: self.render_env(&service.env).map_err(|error| format!("`{field}.env`: {error}"))?,
                    credentials: self.credentials(service.credentials.as_ref(), &field)?,
                },
            ));
        }
        let reporter = Arc::clone(&self.reporter);
        if let Backend::Container(container) = &mut self.backend {
            reporter.log("##[group]Starting the job's containers");
            let started = container.start(&reporter, &job, &services).await;
            reporter.log("##[endgroup]");
            started?;
        }
        Ok(())
    }

    fn credentials(&self, credentials: Option<&RegistryCredentials>, field: &str) -> Result<Option<(String, String)>, String> {
        credentials
            .map(|credentials| {
                let username = self.render(&credentials.username).map_err(|error| format!("`{field}.credentials.username`: {error}"))?;
                let password = self.render(&credentials.password).map_err(|error| format!("`{field}.credentials.password`: {error}"))?;
                Ok((username, password))
            })
            .transpose()
    }

    /// Removes what [`Self::start`] created.
    pub async fn finish(&mut self) {
        if let Backend::Container(container) = &self.backend {
            container.stop(&self.reporter).await;
        }
    }

    /// Runs every step, then evaluates the job's outputs.
    pub async fn run(&mut self) -> TaskResult {
        let minutes = self.spec.template.timeout_minutes;
        let timer = {
            let (interrupt, timed_out, reporter) = (self.interrupt.clone(), Arc::clone(&self.timed_out), Arc::clone(&self.reporter));
            tokio::spawn(async move {
                tokio::select! {
                    () = interrupt.cancelled() => {}
                    () = tokio::time::sleep(Duration::from_secs(u64::from(minutes) * 60)) => {
                        reporter.log(&format!("##[error]The job has exceeded its timeout of {minutes} minutes"));
                        timed_out.store(true, Ordering::SeqCst);
                        interrupt.cancel();
                    }
                }
            })
        };
        let steps = self.spec.template.steps.clone();
        for (index, step) in steps.iter().enumerate() {
            if self.interrupt.is_cancelled() && self.interrupted_at.is_none() {
                self.interrupted_at = Some(Instant::now());
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
        self.post_steps().await;
        timer.abort();
        self.save_caches().await;
        self.outputs();
        self.environment_url();
        if self.interrupt.is_cancelled() {
            self.phase = Phase::Cancelled;
        }
        match self.phase {
            Phase::Success => TaskResult::Success,
            Phase::Failure => TaskResult::Failure,
            // A job that ran out of time failed; one the server or the runner stopped
            // was cancelled.
            Phase::Cancelled if self.timed_out.load(Ordering::SeqCst) => TaskResult::Failure,
            Phase::Cancelled => TaskResult::Cancelled,
        }
    }

    // Actions: third-party actions.
    /// The actions' `post` scripts, newest first, each a step of its own after the job's.
    async fn post_steps(&mut self) {
        let posts = std::mem::take(&mut self.post_steps);
        for post in posts.into_iter().rev() {
            if self.interrupt.is_cancelled() && self.interrupted_at.is_none() {
                self.interrupted_at = Some(Instant::now());
                self.phase = Phase::Cancelled;
                self.sync_contexts();
            }
            let index = self.reporter.add_step(&post.name);
            let text = post.condition.as_deref().unwrap_or("always()");
            let run = Condition::parse(text).and_then(|condition| condition.evaluate(&self.eval(Some(self.phase.status()))));
            match run {
                Ok(true) => {}
                Ok(false) => {
                    self.reporter.finish_step(index, StepStatus::Skipped);
                    continue;
                }
                Err(error) => {
                    self.reporter.start_step(index);
                    self.reporter.log(&format!("##[error]`post-if`: {error}"));
                    self.reporter.finish_step(index, StepStatus::Failure);
                    self.phase = self.phase.max_with(Phase::Failure);
                    continue;
                }
            }
            self.reporter.start_step(index);
            self.reporter.log(&format!("##[group]{}", post.name));
            self.reporter.log("##[endgroup]");
            self.set_env_context(&post.step_env);
            let (outcome, _) = steps::run::execute(self, index, post.process, &post.step_env, None).await;
            self.set_env_context(&BTreeMap::new());
            self.reporter.finish_step(index, outcome.status);
            match outcome.status {
                StepStatus::Failure => self.phase = self.phase.max_with(Phase::Failure),
                StepStatus::Cancelled => self.phase = Phase::Cancelled,
                _ => {}
            }
            self.sync_contexts();
        }
    }

    async fn run_step(&mut self, index: usize, step: &StepTemplate) -> Outcome {
        let step_env = match self.render_env(&step.env) {
            Ok(env) => env,
            Err(error) => return self.fail(&format!("`env`: {error}")),
        };
        self.set_env_context(&step_env);
        let timeout = match self.step_timeout(step) {
            Ok(timeout) => timeout,
            Err(error) => {
                self.set_env_context(&BTreeMap::new());
                return self.fail(&format!("`timeout-minutes`: {error}"));
            }
        };
        let outcome = match &step.action {
            StepAction::Run(script) => steps::run::run(self, index, step, script, &step_env, timeout).await,
            StepAction::Uses(builtin) => steps::builtin(self, builtin, timeout).await,
            StepAction::Action(action) => steps::action::action(self, index, step, action, &step_env, timeout, 0).await,
        };
        self.set_env_context(&BTreeMap::new());
        outcome
    }

    /// A step's `timeout-minutes`, evaluated.
    pub(crate) fn step_timeout(&self, step: &StepTemplate) -> Result<Option<Duration>, String> {
        let Some(text) = step.timeout_minutes.as_deref() else { return Ok(None) };
        let minutes = rbit_workflow::expr::to_number(&self.evaluate(text)?);
        if !(minutes.is_finite() && minutes > 0.0) {
            return Err(format!("`{text}` is not a positive number of minutes"));
        }
        Ok(Some(Duration::from_secs_f64((minutes * 60.0).min(1e9))))
    }

    /// What stops the next step. Before an interrupt: the interrupt, or the step's own
    /// timeout. After it: whichever comes first of the step's timeout and the end of the
    /// shared [`CANCEL_BUDGET`].
    pub fn stop_signal(&self, timeout: Option<Duration>) -> StopSignal {
        let now = Instant::now();
        let step = timeout.map(|timeout| (now + timeout, StopReason::StepTimeout(timeout)));
        let reason = Arc::default();
        match self.interrupted_at {
            None => StopSignal { interrupt: Some(self.interrupt.clone()), deadline: step, reason },
            Some(at) => {
                let budget = (at + CANCEL_BUDGET, StopReason::Budget);
                let deadline = match step {
                    Some(step) if step.0 < budget.0 => step,
                    _ => budget,
                };
                StopSignal { interrupt: None, deadline: Some(deadline), reason }
            }
        }
    }

    /// The outcome of a step that `reason` stopped.
    pub fn stopped(&self, reason: StopReason) -> Outcome {
        match reason {
            StopReason::Cancelled => {
                self.reporter.log("##[error]The step was cancelled");
                Outcome::cancelled()
            }
            StopReason::StepTimeout(timeout) => {
                self.fail(&format!("The step has exceeded its timeout of {} minutes", timeout.as_secs_f64() / 60.0))
            }
            StopReason::Budget => {
                self.reporter.log(&format!(
                    "##[error]Steps after a cancellation may take {} minutes together; this one was stopped",
                    CANCEL_BUDGET.as_secs() / 60
                ));
                Outcome::cancelled()
            }
        }
    }

    /// `<task dir>/private`, created: the runner's own files, which no container mounts.
    pub fn private_dir(&self) -> std::io::Result<std::path::PathBuf> {
        let dir = self.reporter.dir().join("private");
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(dir)
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

    pub(crate) fn continue_on_error(&self, step: &StepTemplate) -> bool {
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

    pub(crate) fn render_env(&self, env: &Env) -> Result<BTreeMap<String, String>, String> {
        env.iter()
            .map(|(name, value)| {
                if !crate::envfiles::valid_variable_name(name) {
                    return Err(format!("`{}` is not a valid variable name", name.escape_debug()));
                }
                Ok((name.clone(), self.render(value).map_err(|error| format!("{name}: {error}"))?))
            })
            .collect()
    }

    /// The `env` context while a step runs: the job's, then the step's own.
    pub(crate) fn set_env_context(&mut self, step_env: &BTreeMap<String, String>) {
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
        env.insert("PATH".to_owned(), self.search_path(step_env));
        env.extend(extra);
        env
    }

    /// `PATH` as a step with `step_env` sees it: the `GITHUB_PATH` entries in front of the
    /// `PATH` its `env` (or the job's) sets, or else the backend's.
    pub fn search_path(&self, step_env: &BTreeMap<String, String>) -> String {
        let separator = self.backend.path_separator().to_string();
        let base = match step_env.get("PATH").or_else(|| self.env.get("PATH")) {
            Some(path) => path.clone(),
            None => self.backend.base_path(),
        };
        self.path.iter().map(String::as_str).chain((!base.is_empty()).then_some(base.as_str())).collect::<Vec<_>>().join(&separator)
    }

    // Actions: environments.
    /// Evaluates and reports the job's `environment.url`, after its steps.
    fn environment_url(&mut self) {
        let Some(template) = self.spec.template.environment_url.clone() else { return };
        match self.render(&template) {
            Ok(url) => self.reporter.set_environment_url(url.trim()),
            Err(error) => self.reporter.log(&format!("##[warning]`environment.url`: {error}")),
        }
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

// Actions: third-party actions.
/// Adds what actions read to `task_env`: where `actions/setup-*` keep the tools they
/// download (the runner's own tool cache on the host, kept between jobs; the task's `temp`
/// in a container, which the job may write) and, when the `ACTIONS_STEP_DEBUG` secret or
/// variable is `true`, `RUNNER_DEBUG`. Returns whether debug logging is on.
fn action_env(
    task_env: &mut BTreeMap<String, String>,
    loaded: &Loaded,
    backend: BackendKind,
    view: &View,
    spec: &JobSpec,
) -> Result<bool, String> {
    let tool_cache = match backend {
        BackendKind::Host => {
            let dir = loaded.work_dir.join("tool-cache");
            std::fs::create_dir_all(&dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
            dir.display().to_string()
        }
        BackendKind::Container => format!("{}/_tool", view.temp),
    };
    for name in ["RUNNER_TOOL_CACHE", "AGENT_TOOLSDIRECTORY"] {
        task_env.insert(name.to_owned(), tool_cache.clone());
    }
    let step_debug = spec
        .secrets
        .get("ACTIONS_STEP_DEBUG")
        .map(|secret| secret.expose().to_owned())
        .or_else(|| spec.contexts.vars.get("ACTIONS_STEP_DEBUG").cloned())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));
    if step_debug {
        task_env.insert("RUNNER_DEBUG".to_owned(), "1".to_owned());
    }
    Ok(step_debug)
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
pub(crate) mod tests;
