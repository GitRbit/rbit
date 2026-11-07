//! Spec templates contracts and original validation.
use super::limits::{Env, JOB_TEMPLATE_VERSION, MAX_SERVICES, MAX_STEPS};
use super::permissions::TokenPermissions;
use super::runtime::{ConcurrencyGroup, Container, RunDefaults, Service, StepAction, StepTemplate};
use super::validation::{valid_environment_name, valid_job_key, validate_env};
use crate::parse::valid_job_id;
use crate::protocol::{ValidationError, ensure, unique, validate_labels, validate_outputs};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// One planned job, stored per job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTemplate {
    /// [`crate::spec::JOB_TEMPLATE_VERSION`].
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

impl Strategy {
    /// The strategy of a job without a matrix.
    pub const SINGLE: Self = Self { fail_fast: true, max_parallel: None, job_index: 0, job_total: 1 };

    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(self.job_index < self.job_total, || "`strategy.job-index` must be below `job-total`".to_owned())?;
        ensure(self.max_parallel != Some(0), || "`max-parallel` must be at least 1".to_owned())
    }
}
