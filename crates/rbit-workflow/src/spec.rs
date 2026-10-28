//! What a job is and what a runner is told (§12.3 and §12.8 of `docs/design/actions.md`).
//!
//! * A [`JobTemplate`] is one planned job (one matrix combination), stored with the job when
//!   its run is created. The fields the server evaluates at creation (§12.4, first row) hold
//!   their results: `name`, `runs-on`, the strategy, job `concurrency`, `timeout-minutes`,
//!   job `continue-on-error`, `container.image` and `services.*.image`. Every other
//!   expression stays a template string, to be evaluated by the server when the job
//!   unblocks (job `if`) or by the runner at step time.
//! * A [`JobSpec`] is the template plus what one task needs: the contexts, secrets and
//!   variables, the task token, where to check out from and which cache scopes to use. The
//!   server assembles it on a claim and sends it once, in a [`crate::protocol::PollResponse`].
//!
//! Both are instructions, so both deny unknown fields and carry a version
//! ([`JOB_TEMPLATE_VERSION`], [`JOB_SPEC_VERSION`]): a reader that ignored a field would run
//! a different job (see [`crate::protocol`] for the rule). Secrets and the token are
//! [`Secret`]s, so printing a spec never prints them. Types only: building templates from a
//! plan and evaluating them belong to [`crate::plan`] and [`crate::expr`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    model::{Permission, Permissions},
    parse::valid_job_id,
    protocol::{TaskId, ValidationError, ensure, unique, valid_task_token, validate_labels, validate_outputs},
    secret::Secret,
};

/// The version of the stored [`JobTemplate`] format.
pub const JOB_TEMPLATE_VERSION: u32 = 1;
/// The version of the [`JobSpec`] format sent to runners.
pub const JOB_SPEC_VERSION: u32 = 1;

/// Most steps in one job.
pub const MAX_STEPS: usize = 256;
/// Most service containers in one job.
pub const MAX_SERVICES: usize = 16;
/// `timeout-minutes` of a job that sets none.
pub const DEFAULT_TIMEOUT_MINUTES: u32 = 360;
/// Most secrets one task receives.
pub const MAX_SECRETS: usize = 1024;
/// Largest secret value, in bytes.
pub const MAX_SECRET_BYTES: usize = 64 * 1024;
/// Longest secret, variable or environment variable name, in bytes.
pub const MAX_NAME_BYTES: usize = 100;
/// Prefixes of the names runners set themselves, which no secret or variable may take
/// (`rbit_core::actions::RESERVED_NAME_PREFIXES`). The task token is [`JobSpec::token`],
/// never a `GITHUB_TOKEN` secret.
pub const RESERVED_NAME_PREFIXES: [&str; 4] = ["GITHUB_", "GITEA_", "FORGEJO_", "RBIT_"];

/// Environment variables by name; the values are templates.
pub type Env = BTreeMap<String, String>;

/// One planned job, stored per job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTemplate {
    /// [`JOB_TEMPLATE_VERSION`].
    pub version: u32,
    /// The key under `jobs`.
    pub job_id: String,
    /// The display name, evaluated.
    pub name: String,
    /// The jobs this one waits for.
    pub needs: Vec<String>,
    /// Job `if`, as written; `None` means `success()`. The server evaluates it at unblock.
    #[serde(rename = "if")]
    pub condition: Option<String>,
    /// `runs-on`, evaluated.
    pub runs_on: Vec<String>,
    /// This job's matrix combination (the `matrix` context); empty without a matrix.
    pub matrix: Map<String, Value>,
    pub strategy: Strategy,
    /// The job's `permissions`, else the workflow's. They only ever lower the repository's
    /// default token permission.
    pub permissions: Option<TokenPermissions>,
    /// Job-level `concurrency`, evaluated.
    pub concurrency: Option<ConcurrencyGroup>,
    /// Job `timeout-minutes`, evaluated and capped by the server.
    pub timeout_minutes: u32,
    /// Job `continue-on-error`, evaluated.
    pub continue_on_error: bool,
    /// Top-level `env`, applied before the job's.
    pub workflow_env: Env,
    /// Job `env`.
    pub env: Env,
    /// `defaults.run`: the job's keys over the workflow's.
    pub defaults: RunDefaults,
    /// Job `outputs`, as written; the runner evaluates them after the last step.
    pub outputs: BTreeMap<String, String>,
    pub container: Option<Container>,
    /// `services`, in file order.
    pub services: Vec<Service>,
    pub steps: Vec<StepTemplate>,
    // Actions: environments.
    /// The deployment environment, by name, evaluated when the job was planned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// `environment.url`, a template the runner evaluates after the steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_url: Option<String>,
    // Actions: reusable workflows.
    /// The calls that led to this job, outermost first; empty for a job of the run's own
    /// workflow. A runner that predates reusable workflows refuses such a template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CallLevel>,
    /// What each name of the `needs` context stands for, in a called workflow or where a
    /// job needs one that calls a workflow. Empty otherwise: then each name of `needs` is
    /// the jobs of that key, as before.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub need_names: BTreeMap<String, NeedSource>,
    // Actions: plans from needs.
    /// A stand-in for a job whose `runs-on`, `strategy`, `container`, `services` or
    /// `environment` reads `needs`: it stays blocked until its needs are done, when the
    /// server plans the job with their results and outputs ([`crate::plan::plan_deferred`])
    /// and puts the jobs it gives in its place. No runner ever gets it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deferred: bool,
    // Actions: reusable workflows.
    /// A called workflow's job whose calling jobs declared no `permissions:`: its token stays
    /// within the repository's default, as the calling job's own token does, whatever this
    /// job declares (GitHub: permissions can only be kept or reduced down the chain).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default_ceiling: bool,
}

// Actions: reusable workflows.
/// One call around a job: what the calling job decides and passes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallLevel {
    /// The calling job's name, `call (linux)`.
    pub name: String,
    /// The calling job's `if:`, decided before the job's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// The calling job's matrix combination and strategy.
    pub matrix: Map<String, Value>,
    pub strategy: Strategy,
    /// What the calling job's `needs` stand for: the `needs` of its `if`, `with` and
    /// `secrets`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub needs: BTreeMap<String, NeedSource>,
    /// The inputs known when the run was planned, typed.
    pub inputs: Map<String, Value>,
    /// The inputs whose `with` reads `needs`: evaluated when the job is handed out.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deferred: BTreeMap<String, DeferredInput>,
    pub secrets: PassedSecrets,
}

/// An input of a called workflow evaluated when its job is handed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredInput {
    /// The `with:` template.
    pub value: String,
    /// `boolean`, `number` or `string`.
    pub kind: String,
}

/// The secrets a calling job passes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PassedSecrets {
    /// `secrets: inherit`: all of the caller's.
    Inherit,
    /// Each by the called workflow's name, as a template over the caller's `secrets`.
    Map(BTreeMap<String, String>),
}

/// What a name of the `needs` context stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NeedSource {
    /// Jobs of the run, by key: a job and its matrix instances.
    Jobs(Vec<String>),
    /// A job that called a workflow: every job of the call, and the called workflow's
    /// `outputs` over a `jobs` context of what its jobs stand for.
    Call { keys: Vec<String>, outputs: BTreeMap<String, String>, jobs: BTreeMap<String, NeedSource> },
}

impl NeedSource {
    /// Every job key this stands for.
    pub fn keys(&self) -> Vec<String> {
        match self {
            Self::Jobs(keys) | Self::Call { keys, .. } => keys.clone(),
        }
    }
}

/// Longest environment name.
pub const MAX_ENVIRONMENT_NAME_BYTES: usize = 255;

/// The `strategy` of a job: what the server enforces and the `strategy` context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strategy {
    /// `fail-fast`, default `true`: a failed job cancels its matrix siblings.
    pub fail_fast: bool,
    /// `max-parallel`: at most this many of the matrix's jobs run at once.
    pub max_parallel: Option<u32>,
    /// This job's position among its matrix's jobs, from 0.
    pub job_index: u32,
    /// How many jobs the matrix has (1 without a matrix).
    pub job_total: u32,
}

/// The coarse token permissions of §11.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPermissions {
    pub contents: Access,
    pub issues: Access,
    pub pull_requests: Access,
}

/// One coarse permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    None,
    Read,
    Write,
}

impl From<Permission> for Access {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::None => Self::None,
            Permission::Read => Self::Read,
            Permission::Write => Self::Write,
        }
    }
}

impl From<Permissions> for TokenPermissions {
    fn from(permissions: Permissions) -> Self {
        Self { contents: permissions.contents.into(), issues: permissions.issues.into(), pull_requests: permissions.pull_requests.into() }
    }
}

/// Job-level `concurrency`, evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyGroup {
    pub group: String,
    pub cancel_in_progress: bool,
}

/// `defaults.run`, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunDefaults {
    pub shell: Option<String>,
    pub working_directory: Option<String>,
}

/// The job `container`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    /// Evaluated; it overrides the image of the runner's label.
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// `options`, evaluated and checked: `docker create` arguments ([`crate::options`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// `volumes`, evaluated and checked: `name:/path[:ro]`, named volumes only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

/// One `services.<id>` container (container backend only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// The key under `services`, also its network alias.
    pub id: String,
    /// Evaluated.
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// As [`Container::options`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// As [`Container::volumes`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

/// `credentials` of a container or service, as templates (normally
/// `${{ secrets.REGISTRY_PASSWORD }}`). The runner evaluates them and logs in with
/// `--password-stdin`. `Debug` hides the password in case it was written out literally.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCredentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for RegistryCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryCredentials").field("username", &self.username).field("password", &"***").finish()
    }
}

/// One step. Everything here is a template the runner evaluates at step time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepTemplate {
    /// `id`, for the `steps` context.
    pub id: Option<String>,
    pub name: Option<String>,
    /// `if`; `None` means `success()`.
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub action: StepAction,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub env: Env,
    /// `continue-on-error` as written (`true`, or an expression).
    pub continue_on_error: Option<String>,
    /// `timeout-minutes` as written (a number, or an expression).
    pub timeout_minutes: Option<String>,
}

/// What a step does: exactly one of `run` and `uses`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StepAction {
    /// A `run:` script, interpolated as text and written to a file.
    Run(String),
    /// A built-in action.
    Uses(Builtin),
    // Actions: third-party actions.
    /// An action the runner fetches and runs.
    Action(ActionStep),
}

// Actions: third-party actions.
/// A `uses:` that is not built in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionStep {
    /// `uses:` as written, for display.
    pub uses: String,
    pub source: ActionSource,
    /// `with:`, as templates; the runner checks them against the action's inputs.
    pub with: BTreeMap<String, String>,
}

/// Where an action comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionSource {
    /// `owner/repo[/path]@reference`, from the runner's action source.
    Repository { owner: String, repo: String, path: String, reference: String },
    /// `./path`: a directory of the workspace.
    Local { path: String },
    /// `docker://image`.
    Docker { image: String },
}

/// A resolved `uses:` (§12.2 built-ins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Builtin {
    /// `uses:` as written (`actions/checkout@v4`), for display.
    pub uses: String,
    pub kind: BuiltinKind,
    /// The major version of the rbit built-in it resolved to.
    pub version: u32,
    /// `with:`, as templates; the keys were checked against the built-in.
    pub with: BTreeMap<String, String>,
}

/// The built-in actions `rbit-runner` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinKind {
    /// `rbit/checkout`.
    Checkout,
    /// `rbit/cache`: restore now, save after the job.
    Cache,
    /// `actions/cache/restore`.
    CacheRestore,
    /// `actions/cache/save`.
    CacheSave,
    /// `rbit/upload-artifact`.
    UploadArtifact,
    /// `rbit/download-artifact`.
    DownloadArtifact,
}

// Actions: reusable workflows.
/// A job's key in its run: a job id, or for a called workflow's job the calling jobs' ids
/// and its own, `/`-separated, a calling matrix job's with `#<index>` (`call#1/build`); at
/// most [`crate::plan::MAX_JOB_KEY_BYTES`].
pub fn valid_job_key(key: &str) -> bool {
    key.len() <= crate::plan::MAX_JOB_KEY_BYTES
        && key.split('/').all(|part| {
            let id = match part.rsplit_once('#') {
                Some((id, index)) if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()) => id,
                Some(_) => return false,
                None => part,
            };
            valid_job_id(id)
        })
}

impl JobTemplate {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.version == JOB_TEMPLATE_VERSION, || format!("job template version {} is not supported", self.version))?;
        ensure(valid_job_key(&self.job_id), || format!("job id `{}` is malformed", self.job_id))?;
        for need in &self.needs {
            ensure(valid_job_key(need) && *need != self.job_id, || format!("`needs` entry `{need}` is malformed"))?;
        }
        ensure(unique(self.needs.iter()), || "`needs` must not repeat".to_owned())?;
        validate_labels(self.runs_on.iter().map(String::as_str))?;
        self.strategy.validate()?;
        ensure(self.timeout_minutes >= 1, || "`timeout-minutes` must be at least 1".to_owned())?;
        validate_env(&self.workflow_env)?;
        validate_env(&self.env)?;
        validate_outputs(&self.outputs)?;
        if let Some(environment) = &self.environment {
            ensure(valid_environment_name(environment), || format!("environment `{}` is malformed", environment.escape_debug()))?;
        }
        if let Some(container) = &self.container {
            ensure(!container.image.is_empty(), || "`container.image` must not be empty".to_owned())?;
            validate_env(&container.env)?;
        }
        ensure(self.services.len() <= MAX_SERVICES, || format!("a job has at most {MAX_SERVICES} services"))?;
        ensure(unique(self.services.iter().map(|service| &service.id)), || "service ids must not repeat".to_owned())?;
        for service in &self.services {
            ensure(valid_job_id(&service.id), || format!("service id `{}` is malformed", service.id))?;
            ensure(!service.image.is_empty(), || format!("service `{}` has no image", service.id))?;
            validate_env(&service.env)?;
        }
        ensure((1..=MAX_STEPS).contains(&self.steps.len()), || format!("a job has 1 to {MAX_STEPS} steps"))?;
        ensure(unique(self.steps.iter().filter_map(|step| step.id.as_ref())), || "step ids must not repeat".to_owned())?;
        for step in &self.steps {
            if let Some(id) = &step.id {
                ensure(valid_job_id(id), || format!("step id `{id}` is malformed"))?;
            }
            if let StepAction::Uses(builtin) = &step.action {
                ensure(builtin.version >= 1, || format!("`{}` has no version", builtin.uses))?;
            }
            // Actions: third-party actions. The runner fetches what this names, so it is
            // checked again here, whoever wrote the template.
            if let StepAction::Action(action) = &step.action {
                let resolved = crate::builtins::resolve_uses(&action.uses);
                ensure(resolved == Ok(crate::builtins::Uses::Action(action.source.clone())), || {
                    format!("`{}` does not name the action it carries", action.uses)
                })?;
            }
            validate_env(&step.env)?;
        }
        Ok(())
    }
}

impl Strategy {
    /// The strategy of a job without a matrix.
    pub const SINGLE: Self = Self { fail_fast: true, max_parallel: None, job_index: 0, job_total: 1 };

    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.job_index < self.job_total, || "`strategy.job-index` must be below `job-total`".to_owned())?;
        ensure(self.max_parallel != Some(0), || "`max-parallel` must be at least 1".to_owned())
    }
}

// Actions: environments.
/// An environment's name: 1 to [`MAX_ENVIRONMENT_NAME_BYTES`] bytes, no control
/// characters, `/` or `\\`, and not blank at either end. GitHub allows little more.
pub fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ENVIRONMENT_NAME_BYTES
        && name.trim() == name
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
}

/// Environment variable names: 1 to [`MAX_NAME_BYTES`], without `=` or NUL.
fn validate_env(env: &Env) -> Result<(), ValidationError> {
    for name in env.keys() {
        ensure(!name.is_empty() && name.len() <= MAX_NAME_BYTES && !name.contains(['=', '\0']), || {
            format!("environment variable name `{name}` is malformed")
        })?;
    }
    Ok(())
}

/// What the server sends a runner for one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpec {
    /// [`JOB_SPEC_VERSION`].
    pub version: u32,
    pub task_id: TaskId,
    pub template: JobTemplate,
    pub contexts: Contexts,
    /// Repository then owner secrets, by name. Empty for untrusted runs.
    pub secrets: BTreeMap<String, Secret>,
    /// The task token: `secrets.GITHUB_TOKEN`, `secrets.RBIT_TOKEN` and `github.token`.
    /// It reaches a step's environment only where the step maps it.
    pub token: Secret,
    pub checkout: CheckoutSource,
    pub cache: CacheScopes,
    // Actions: settings API.
    /// The actions its repository and owner allow. Sent only when it restricts something:
    /// a runner that does not know the field refuses the whole spec rather than ignore it.
    #[serde(default, skip_serializing_if = "ActionsPolicy::is_open")]
    pub actions_policy: ActionsPolicy,
}

// Actions: settings API.
/// Which actions a job may use besides rbit's built-ins and the workspace's own, from its
/// repository's and owner's `allowed_actions`: an action runs only when every rule allows
/// it, on top of the runner's own `[actions] allow`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsPolicy {
    pub rules: Vec<ActionsRule>,
}

/// One owner's or repository's rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionsRule {
    /// No action from outside the repository.
    LocalOnly,
    /// GitHub's own actions (`actions/*`, `github/*`) when `github_owned`, and those a
    /// pattern matches: `owner/repo[/path]@ref` or, without `@`, any ref; `*` matches any
    /// run of characters; `docker://image` patterns match Docker images. Case is ignored.
    Selected { github_owned: bool, patterns: Vec<String> },
}

impl ActionsPolicy {
    pub fn is_open(&self) -> bool {
        self.rules.is_empty()
    }

    /// The first rule that refuses `source`, or `None` when it may run. The workspace's
    /// own actions always may.
    pub fn refuses(&self, source: &ActionSource) -> Option<&ActionsRule> {
        self.rules.iter().find(|rule| !rule.allows(source))
    }
}

impl ActionsRule {
    fn allows(&self, source: &ActionSource) -> bool {
        match (self, source) {
            (_, ActionSource::Local { .. }) => true,
            (Self::LocalOnly, _) => false,
            (Self::Selected { github_owned, patterns }, ActionSource::Repository { owner, repo, path, reference }) => {
                let owner = owner.to_ascii_lowercase();
                if *github_owned && (owner == "actions" || owner == "github") {
                    return true;
                }
                let name = if path.is_empty() { format!("{owner}/{repo}") } else { format!("{owner}/{repo}/{path}") }.to_ascii_lowercase();
                let full = format!("{name}@{}", reference.to_ascii_lowercase());
                patterns.iter().map(|pattern| pattern.to_ascii_lowercase()).any(|pattern| {
                    !pattern.starts_with("docker://")
                        && if pattern.contains('@') { wildcard(&pattern, &full) } else { wildcard(&pattern, &name) }
                })
            }
            (Self::Selected { patterns, .. }, ActionSource::Docker { image }) => {
                let image = format!("docker://{}", image.to_ascii_lowercase());
                patterns.iter().any(|pattern| wildcard(&pattern.to_ascii_lowercase(), &image))
            }
        }
    }
}

/// Whether `text` matches `pattern`, where `*` stands for any run of characters.
fn wildcard(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    // The classic greedy match with one backtrack point: linear for these short strings.
    let (mut p, mut t, mut star, mut mark) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && pattern[p] != b'*' && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            mark = t;
        } else if let Some(at) = star {
            p = at + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&byte| byte == b'*')
}

/// The contexts the server supplies. `matrix` and `strategy` come from the template; `env`,
/// `steps`, `job`, `runner` and `secrets` are the runner's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contexts {
    /// The `github` context, `event` included. It never holds `token`: that is
    /// [`JobSpec::token`], which stays redacted.
    pub github: Map<String, Value>,
    /// `inputs` of a `workflow_dispatch` run; empty otherwise.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables. Empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
    /// One entry for every job in [`JobTemplate::needs`].
    pub needs: BTreeMap<String, Needed>,
}

/// One entry of the `needs` context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Needed {
    pub result: JobResult,
    /// Outputs, merged across the job's matrix instances.
    pub outputs: BTreeMap<String, String>,
}

/// `needs.<id>.result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobResult {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

/// Where the built-in checkout fetches from (§12.5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutSource {
    /// The clone URL, `<server>/<owner>/<repo>`, without credentials.
    pub url: String,
    /// `owner/repo`.
    pub repository: String,
    /// The commit the run is for.
    pub sha: String,
    /// The full ref (`refs/heads/main`, `refs/pull/3/head`).
    #[serde(rename = "ref")]
    pub reference: String,
}

/// The runner-local cache scopes this task may use (§12.5.6), decided by the server from
/// the run's ref and trust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheScopes {
    pub repository_id: i64,
    /// Scopes to restore from, in order: the run's own, then the default branch's.
    pub read: Vec<String>,
    /// The one scope saves go to (`pull/<n>` for an untrusted run).
    pub write: String,
}

impl JobSpec {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.version == JOB_SPEC_VERSION, || format!("job spec version {} is not supported", self.version))?;
        self.template.validate()?;
        ensure(self.secrets.len() <= MAX_SECRETS, || format!("a task has at most {MAX_SECRETS} secrets"))?;
        for (name, value) in &self.secrets {
            validate_secret_name("secret", name)?;
            ensure(value.len() <= MAX_SECRET_BYTES, || format!("secret `{name}` is larger than {MAX_SECRET_BYTES} bytes"))?;
        }
        for name in self.contexts.vars.keys() {
            validate_secret_name("variable", name)?;
        }
        ensure(valid_task_token(self.token.expose()), || "the task token is malformed".to_owned())?;
        ensure(!self.contexts.github.keys().any(|key| key.eq_ignore_ascii_case("token")), || {
            "the `github` context must not carry the token".to_owned()
        })?;
        ensure(self.contexts.needs.keys().eq(self.template.needs.iter().collect::<std::collections::BTreeSet<_>>()), || {
            "the `needs` context must have one entry for every needed job".to_owned()
        })?;
        for needed in self.contexts.needs.values() {
            validate_outputs(&needed.outputs)?;
        }
        self.checkout.validate()?;
        self.cache.validate()
    }
}

/// `[A-Z_][A-Z0-9_]*`, at most [`MAX_NAME_BYTES`], with no [`RESERVED_NAME_PREFIXES`] entry:
/// the stored, upper-cased form of secret and variable names.
fn validate_secret_name(kind: &str, name: &str) -> Result<(), ValidationError> {
    let mut bytes = name.bytes();
    let well_formed = name.len() <= MAX_NAME_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_uppercase() || first == b'_')
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    ensure(well_formed, || format!("{kind} name `{name}` is malformed"))?;
    ensure(!RESERVED_NAME_PREFIXES.iter().any(|prefix| name.starts_with(prefix)), || format!("{kind} name `{name}` is reserved"))
}

impl CheckoutSource {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.url.starts_with("https://") || self.url.starts_with("http://"), || "the checkout URL must be HTTP(S)".to_owned())?;
        ensure(!self.url.contains('@'), || "the checkout URL must not carry credentials".to_owned())?;
        ensure(
            self.repository.split_once('/').is_some_and(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/')),
            || "the checkout repository must be `owner/name`".to_owned(),
        )?;
        ensure(matches!(self.sha.len(), 40 | 64) && self.sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), || {
            "the checkout SHA must be a full lowercase object id".to_owned()
        })?;
        ensure(self.reference.starts_with("refs/"), || "the checkout ref must be a full ref".to_owned())
    }
}

impl CacheScopes {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let valid = |scope: &String| {
            !scope.is_empty()
                && scope.len() <= 255
                && !scope.split('/').any(|part| part.is_empty() || part == "." || part == "..")
                && !scope.contains(['\\', '\0'])
        };
        ensure(valid(&self.write) && self.read.iter().all(valid), || "a cache scope is malformed".to_owned())
    }
}

#[cfg(test)]
mod policy_tests {
    use super::{ActionSource, ActionsPolicy, ActionsRule};

    fn repository(owner: &str, repo: &str, path: &str, reference: &str) -> ActionSource {
        ActionSource::Repository { owner: owner.into(), repo: repo.into(), path: path.into(), reference: reference.into() }
    }

    #[test]
    fn every_rule_must_allow_an_action() {
        let open = ActionsPolicy::default();
        assert!(open.is_open() && open.refuses(&repository("evil", "x", "", "v1")).is_none());
        let local = ActionsPolicy { rules: vec![ActionsRule::LocalOnly] };
        assert!(local.refuses(&ActionSource::Local { path: "x".into() }).is_none(), "the workspace's own always run");
        assert!(local.refuses(&repository("actions", "setup-node", "", "v4")).is_some());
        assert!(local.refuses(&ActionSource::Docker { image: "alpine".into() }).is_some());

        let selected = ActionsRule::Selected {
            github_owned: true,
            patterns: vec!["octo-org/*".into(), "Deploy/Tool@V2".into(), "one/repo/sub@*".into(), "docker://alpine:*".into()],
        };
        let policy = ActionsPolicy { rules: vec![selected.clone()] };
        for allowed in [
            repository("actions", "setup-node", "", "v4"),
            repository("github", "codeql-action", "init", "v3"),
            repository("octo-org", "anything", "deep/path", "main"),
            repository("deploy", "tool", "", "v2"),
            repository("one", "repo", "sub", "abc123"),
        ] {
            assert!(policy.refuses(&allowed).is_none(), "{allowed:?}");
        }
        for refused in [repository("deploy", "tool", "", "v3"), repository("one", "repo", "", "v1"), repository("evil", "x", "", "v1")] {
            assert!(policy.refuses(&refused).is_some(), "{refused:?}");
        }
        assert!(policy.refuses(&ActionSource::Docker { image: "alpine:3.20".into() }).is_none());
        assert!(policy.refuses(&ActionSource::Docker { image: "ubuntu".into() }).is_some());

        let both =
            ActionsPolicy { rules: vec![selected, ActionsRule::Selected { github_owned: false, patterns: vec!["octo-org/*".into()] }] };
        assert!(both.refuses(&repository("actions", "setup-node", "", "v4")).is_some(), "the owner's rule refuses it");
        assert!(both.refuses(&repository("octo-org", "x", "", "v1")).is_none());
    }

    #[test]
    fn a_restricted_spec_names_its_policy_and_an_open_one_does_not() {
        let rule = serde_json::to_string(&ActionsPolicy { rules: vec![ActionsRule::LocalOnly] }).unwrap();
        assert_eq!(rule, r#"{"rules":["local_only"]}"#);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{PollResponse, tests::Refusal};

    const TOKEN: &str = "rbat_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    #[test]
    fn called_jobs_have_keys_of_their_calls() {
        for key in ["build", "call/build", "call#1/build", "outer/inner#0/leaf"] {
            assert!(valid_job_key(key), "{key}");
        }
        for key in ["", "call/", "/build", "call#/build", "call#x/build", "a b", &"x/".repeat(60)] {
            assert!(!valid_job_key(key), "{key}");
        }
    }
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn template() -> JobTemplate {
        JobTemplate {
            version: JOB_TEMPLATE_VERSION,
            job_id: "test".into(),
            name: "test (linux, stable)".into(),
            needs: vec!["build".into()],
            condition: Some("${{ success() && github.ref == 'refs/heads/main' }}".into()),
            runs_on: vec!["ubuntu-latest".into()],
            matrix: json!({"os": "linux", "rust": "stable"}).as_object().cloned().unwrap(),
            strategy: Strategy { fail_fast: true, max_parallel: Some(2), job_index: 0, job_total: 4 },
            permissions: Some(TokenPermissions { contents: Access::Read, issues: Access::None, pull_requests: Access::Write }),
            concurrency: Some(ConcurrencyGroup { group: "ci-refs/heads/main".into(), cancel_in_progress: true }),
            timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
            continue_on_error: false,
            workflow_env: Env::from([("CARGO_TERM_COLOR".into(), "always".into())]),
            env: Env::from([("RUST".into(), "${{ matrix.rust }}".into())]),
            defaults: RunDefaults { shell: Some("bash".into()), working_directory: None },
            outputs: BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]),
            container: Some(Container {
                image: "docker.io/library/rust:1.94".into(),
                env: Env::new(),
                credentials: Some(RegistryCredentials { username: "ci".into(), password: "${{ secrets.REGISTRY_PASSWORD }}".into() }),
                options: Vec::new(),
                volumes: Vec::new(),
            }),
            services: vec![Service {
                id: "postgres".into(),
                image: "postgres:17".into(),
                env: Env::from([("POSTGRES_PASSWORD".into(), "pw".into())]),
                credentials: None,
                options: Vec::new(),
                volumes: Vec::new(),
            }],
            steps: vec![
                StepTemplate {
                    id: None,
                    name: None,
                    condition: None,
                    action: StepAction::Uses(Builtin {
                        uses: "actions/checkout@v4".into(),
                        kind: BuiltinKind::Checkout,
                        version: 1,
                        with: BTreeMap::from([("fetch-depth".into(), "0".into())]),
                    }),
                    shell: None,
                    working_directory: None,
                    env: Env::new(),
                    continue_on_error: None,
                    timeout_minutes: None,
                },
                StepTemplate {
                    id: Some("meta".into()),
                    name: Some("Test on ${{ matrix.os }}".into()),
                    condition: Some("always()".into()),
                    action: StepAction::Run("cargo test --locked\necho \"version=1\" >> \"$GITHUB_OUTPUT\"\n".into()),
                    shell: Some("bash".into()),
                    working_directory: Some("crates".into()),
                    env: Env::from([("TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())]),
                    continue_on_error: Some("${{ matrix.rust == 'nightly' }}".into()),
                    timeout_minutes: Some("30".into()),
                },
            ],
            environment: None,
            environment_url: None,
            calls: Vec::new(),
            need_names: std::collections::BTreeMap::new(),
            deferred: false,
            default_ceiling: false,
        }
    }

    fn spec() -> JobSpec {
        JobSpec {
            version: JOB_SPEC_VERSION,
            task_id: TaskId(42),
            template: template(),
            contexts: Contexts {
                github: json!({
                    "event_name": "push",
                    "ref": "refs/heads/main",
                    "sha": SHA,
                    "repository": "alice/app",
                    "run_id": "9",
                    "server_url": "https://git.example.com",
                    "event": {"ref": "refs/heads/main"}
                })
                .as_object()
                .cloned()
                .unwrap(),
                inputs: Map::new(),
                vars: BTreeMap::from([("DEPLOY_HOST".into(), "deploy.example.com".into())]),
                needs: BTreeMap::from([(
                    "build".into(),
                    Needed { result: JobResult::Success, outputs: BTreeMap::from([("artifact".into(), "app.tar".into())]) },
                )]),
            },
            secrets: BTreeMap::from([("REGISTRY_PASSWORD".into(), Secret::new("s3cr3t-registry"))]),
            token: Secret::new(TOKEN),
            checkout: CheckoutSource {
                url: "https://git.example.com/alice/app".into(),
                repository: "alice/app".into(),
                sha: SHA.into(),
                reference: "refs/heads/main".into(),
            },
            cache: CacheScopes { repository_id: 5, read: vec!["refs/heads/main".into()], write: "refs/heads/main".into() },
            actions_policy: ActionsPolicy::default(),
        }
    }

    /// Pretty JSON equals the golden file and parses back to the value. `RBIT_BLESS=1`
    /// rewrites the file after a deliberate change.
    #[track_caller]
    fn golden_file<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T, name: &str, expected: &str) {
        let actual = serde_json::to_string_pretty(value).unwrap() + "\n";
        if std::env::var_os("RBIT_BLESS").is_some() {
            std::fs::write(format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR")), &actual).unwrap();
            return;
        }
        assert_eq!(actual, expected.replace("\r\n", "\n"), "{name}");
        assert_eq!(&serde_json::from_str::<T>(expected).unwrap(), value, "{name}");
    }

    #[test]
    fn golden_template_and_spec() {
        template().validate().unwrap();
        spec().validate().unwrap();
        golden_file(&template(), "job_template.json", include_str!("../tests/golden/job_template.json"));
        golden_file(&PollResponse { task: spec() }, "poll_response.json", include_str!("../tests/golden/poll_response.json"));
    }

    #[test]
    fn specs_are_strict() {
        let mut value = serde_json::to_value(spec()).unwrap();
        value["template"]["steps"][1]["privileged"] = json!(true);
        assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown step field");
        let mut value = serde_json::to_value(spec()).unwrap();
        value["template"]["steps"][0]["action"] = json!({"run": "a", "uses": {}});
        assert!(serde_json::from_value::<JobSpec>(value).is_err(), "both run and uses");
        let mut value = serde_json::to_value(spec()).unwrap();
        value["template"]["steps"][0]["action"]["uses"]["kind"] = json!("setup_node");
        assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown built-in");
        let mut value = serde_json::to_value(spec()).unwrap();
        value["sandbox"] = json!(false);
        assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown top-level field");
        let mut value = serde_json::to_value(spec()).unwrap();
        value.as_object_mut().unwrap().remove("cache");
        assert!(serde_json::from_value::<JobSpec>(value).is_err(), "a missing field");
    }

    #[test]
    fn template_validation() {
        let refused: [Refusal<JobTemplate>; 16] = [
            (|t| t.version = 2, "version 2"),
            (|t| t.job_id = "1st".into(), "job id"),
            (|t| t.needs.push("test".into()), "`needs` entry `test`"),
            (|t| t.needs.push("build".into()), "must not repeat"),
            (|t| t.runs_on.clear(), "labels"),
            (|t| t.runs_on.push("ubuntu latest".into()), "label `ubuntu latest`"),
            (|t| t.strategy.job_index = 4, "job-index"),
            (|t| t.strategy.max_parallel = Some(0), "max-parallel"),
            (|t| t.timeout_minutes = 0, "timeout-minutes"),
            (|t| _ = t.env.insert("A=B".into(), String::new()), "`A=B`"),
            (|t| _ = t.outputs.insert("1x".into(), String::new()), "output name"),
            (|t| t.services.push(t.services[0].clone()), "service ids"),
            (|t| t.services[0].image.clear(), "no image"),
            (|t| t.steps.clear(), "1 to 256 steps"),
            (|t| t.steps[0].id = Some("meta".into()), "step ids"),
            (|t| t.steps[0].id = Some("a b".into()), "step id `a b`"),
        ];
        for (change, message) in refused {
            let mut template = template();
            change(&mut template);
            let error = template.validate().unwrap_err().message;
            assert!(error.contains(message), "{error:?} should mention {message:?}");
        }
        let mut long = template();
        long.steps = vec![long.steps[0].clone(); MAX_STEPS + 1];
        assert!(long.validate().is_err());
        long.steps.truncate(MAX_STEPS);
        long.validate().unwrap();
        let mut many = template();
        many.services = (0..=MAX_SERVICES).map(|i| Service { id: format!("s{i}"), ..many.services[0].clone() }).collect();
        assert!(many.validate().is_err());
    }

    #[test]
    fn spec_validation() {
        let refused: [Refusal<JobSpec>; 13] = [
            (|s| s.version = 0, "version 0"),
            (|s| s.template.version = 0, "job template version"),
            (|s| _ = s.secrets.insert("GITHUB_TOKEN".into(), Secret::new("x")), "`GITHUB_TOKEN` is reserved"),
            (|s| _ = s.secrets.insert("lower".into(), Secret::new("x")), "`lower` is malformed"),
            (|s| _ = s.secrets.insert("BIG".into(), Secret::new("x".repeat(MAX_SECRET_BYTES + 1))), "`BIG` is larger"),
            (|s| _ = s.contexts.vars.insert("RBIT_X".into(), String::new()), "variable name `RBIT_X` is reserved"),
            (|s| s.token = Secret::new("rbit_notatasktoken"), "task token"),
            (|s| _ = s.contexts.github.insert("Token".into(), json!("x")), "must not carry the token"),
            (|s| s.contexts.needs.clear(), "every needed job"),
            (|s| s.checkout.url = "https://user:pw@git.example.com/alice/app".into(), "credentials"),
            (|s| s.checkout.sha = SHA.to_uppercase(), "SHA"),
            (|s| s.checkout.reference = "main".into(), "full ref"),
            (|s| s.cache.write = "../other".into(), "cache scope"),
        ];
        for (change, message) in refused {
            let mut spec = spec();
            change(&mut spec);
            let error = spec.validate().unwrap_err().message;
            assert!(error.contains(message), "{error:?} should mention {message:?}");
        }
        let mut sha256 = spec();
        sha256.checkout.sha = "ab".repeat(32);
        sha256.validate().unwrap();
    }

    #[test]
    fn secrets_never_appear_in_debug_output() {
        let debug = format!("{:?} {:?}", spec(), PollResponse { task: spec() });
        assert!(!debug.contains("s3cr3t-registry") && !debug.contains(TOKEN), "{debug}");
        let credentials = RegistryCredentials { username: "ci".into(), password: "literal-password".into() };
        assert!(!format!("{credentials:?}").contains("literal-password"));
    }

    #[test]
    fn permissions_convert_from_the_model() {
        let model = Permissions { contents: Permission::Write, issues: Permission::Read, pull_requests: Permission::None };
        let converted = TokenPermissions::from(model);
        assert_eq!(converted, TokenPermissions { contents: Access::Write, issues: Access::Read, pull_requests: Access::None });
        assert_eq!(serde_json::to_string(&converted).unwrap(), r#"{"contents":"write","issues":"read","pull_requests":"none"}"#);
        assert_eq!(Strategy::SINGLE.validate(), Ok(()));
    }
}
