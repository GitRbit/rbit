//! Reusable workflows on the server: finding the workflows that jobs call, and what a called
//! job gets when it is decided and handed out.
//!
//! * [`CallLoader`] is the planner's [`Loader`]. `./path` is read from the repository and
//!   commit of the workflow that calls it; `owner/repo/path@ref` from a repository of this
//!   instance at that branch, tag or commit. The run's own repository may always be called;
//!   another public one too; another private one only by a private repository of the same
//!   owner, and only once its `access_level` (GitHub's `actions/permissions/access`) is
//!   `user` or `organization`. Every refusal reads the same, so a private repository is
//!   never told apart from a missing one.
//! * A called job's `inputs` are those its calls typed when the run was planned, plus the
//!   `with:` that read `needs`, evaluated level by level when the job is decided or handed
//!   out ([`ActionsService::call_inputs`]). Its secrets pass through every call: all of them
//!   with `secrets: inherit`, otherwise only the ones the caller names
//!   ([`ActionsService::call_secrets`]).
//! * A `needs` name that stands for a job that called a workflow has the result of all its
//!   jobs and the called workflow's `outputs`, evaluated over what its jobs give
//!   ([`need`]).

use std::collections::{BTreeMap, HashMap};

use rbit_core::{
    RepoId, Timestamp, User,
    actions::{Run, Status},
    repository::OwnedRepository,
};
use rbit_git::RepositoryReader;
use rbit_workflow::{
    Deadline, Limits, Loaded, Loader,
    expr::{ContextMap, Status as ExprStatus, Template, to_text},
    model::{CallInputKind, CallTarget},
    parse::call_input_value,
    spec::{CallLevel, JobResult, JobTemplate, NeedSource, Needed, PassedSecrets},
};
use serde_json::{Map, Value, json};
use tokio::runtime::Handle;

use super::{ActionsService, triggers::read_workflow_file};
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};

/// The access levels GitHub's `actions/permissions/access` takes.
pub const ACCESS_LEVELS: [&str; 3] = ["none", "user", "organization"];

/// The planner's [`Loader`] on the server. Blocking: runs inside an evaluation.
pub(super) struct CallLoader<'a> {
    pub handle: &'a Handle,
    pub service: &'a ActionsService,
    /// The run's repository, its reader and commit.
    pub caller: &'a OwnedRepository,
    pub reader: &'a RepositoryReader,
    pub sha: &'a str,
    pub limits: &'a Limits,
    pub deadline: &'a Deadline,
    /// Repositories read so far, by id.
    pub opened: HashMap<RepoId, (OwnedRepository, RepositoryReader)>,
}

impl<'a> CallLoader<'a> {
    pub fn new(
        handle: &'a Handle,
        service: &'a ActionsService,
        caller: &'a OwnedRepository,
        reader: &'a RepositoryReader,
        sha: &'a str,
        limits: &'a Limits,
        deadline: &'a Deadline,
    ) -> Self {
        Self { handle, service, caller, reader, sha, limits, deadline, opened: HashMap::new() }
    }

    fn reader_of(&mut self, repo: RepoId) -> Option<RepositoryReader> {
        if repo == self.caller.repo.id {
            return Some(self.reader.clone());
        }
        if let Some((_, reader)) = self.opened.get(&repo) {
            return Some(reader.clone());
        }
        let owned = self.handle.block_on(self.service.ctx().database.admin().find_repository(repo)).ok().flatten()?;
        let reader = self.service.reader(&owned);
        self.opened.insert(repo, (owned, reader.clone()));
        Some(reader)
    }

    /// Whether the run's repository may call workflows of `called`.
    fn may_call(&self, called: &OwnedRepository) -> bool {
        if called.repo.id == self.caller.repo.id || called.repo.visibility.is_public() {
            return true;
        }
        if self.caller.repo.visibility.is_public() || called.repo.owner_id != self.caller.repo.owner_id {
            return false;
        }
        let level = self.handle.block_on(self.service.ctx().database.actions_settings().access_level(called.repo.id));
        matches!(level.as_deref(), Ok("user" | "organization"))
    }

    fn read(&mut self, repo: RepoId, sha: &str, path: &str, shown: &str) -> Result<Loaded, String> {
        let reader = self.reader_of(repo).ok_or_else(|| unavailable(shown))?;
        let workflow = read_workflow_file(self.handle, &reader, sha, path, self.limits, self.deadline).map_err(|error| {
            // A file that is not there reads like a repository that may not be called.
            if error.to_string().contains("could not be read") { unavailable(shown) } else { format!("`{shown}`: {error}") }
        })?;
        let origin = format!("{}@{sha}", repo.0);
        Ok(Loaded { workflow, id: format!("{origin}:{path}"), origin })
    }
}

impl Loader for CallLoader<'_> {
    fn load(&mut self, origin: Option<&str>, target: &CallTarget) -> Result<Loaded, String> {
        match target {
            CallTarget::Local { path } => {
                let (repo, sha) = match origin.and_then(|origin| origin.split_once('@')) {
                    Some((repo, sha)) => (RepoId(repo.parse().map_err(|_| unavailable(path))?), sha.to_owned()),
                    None => (self.caller.repo.id, self.sha.to_owned()),
                };
                self.read(repo, &sha, path, &format!("./{path}"))
            }
            CallTarget::Remote { owner, repo, path, reference } => {
                let shown = format!("{owner}/{repo}/{path}@{reference}");
                let found = self
                    .handle
                    .block_on(self.service.ctx().database.repositories().find_by_path(owner, repo))
                    .ok()
                    .flatten()
                    .filter(|called| self.may_call(called))
                    .ok_or_else(|| unavailable(&shown))?;
                let reader = self.service.reader(&found);
                let sha = [format!("refs/heads/{reference}"), format!("refs/tags/{reference}"), reference.clone()]
                    .into_iter()
                    .find_map(|name| self.handle.block_on(reader.commit_id(name)).ok())
                    .ok_or_else(|| format!("`{shown}`: `{reference}` is no branch, tag or commit of the repository"))?;
                let id = found.repo.id;
                self.opened.insert(id, (found, reader));
                self.read(id, &sha, path, &shown)
            }
        }
    }
}

fn unavailable(shown: &str) -> String {
    format!("`{shown}` is not there, or this repository may not call it")
}

impl ActionsService {
    /// The `inputs` of a called job: each call's, level by level, with the `with:` that read
    /// `needs` evaluated now. The run's own inputs for a job of the run's own workflow.
    pub(super) async fn call_inputs(
        &self,
        run: &Run,
        template: &JobTemplate,
        github: &Map<String, Value>,
        vars: &Map<String, Value>,
        run_inputs: Map<String, Value>,
    ) -> Result<Result<Map<String, Value>, String>, ServiceError> {
        let mut outer = run_inputs;
        if template.calls.is_empty() {
            return Ok(Ok(outer));
        }
        let jobs = self.run_jobs_with_outputs(run).await?;
        for level in &template.calls {
            let mut inputs = level.inputs.clone();
            if !level.deferred.is_empty() {
                let contexts = level_contexts(level, github, vars, &outer, &jobs);
                for (name, deferred) in &level.deferred {
                    let rendered = match Template::parse(&deferred.value).and_then(|template| template.render(&contexts)) {
                        Ok(rendered) => rendered,
                        Err(error) => return Ok(Err(format!("`{}`: input `{name}`: {error}", level.name))),
                    };
                    let kind = match deferred.kind.as_str() {
                        "boolean" => CallInputKind::Boolean,
                        "number" => CallInputKind::Number,
                        _ => CallInputKind::String,
                    };
                    let Some(value) = call_input_value(kind, &rendered) else {
                        return Ok(Err(format!("`{}`: input `{name}`: `{rendered}` is not a {}", level.name, kind.as_str())));
                    };
                    inputs.insert(name.clone(), value);
                }
            }
            outer = inputs;
        }
        Ok(Ok(outer))
    }

    /// The secrets a called job gets from `secrets` (the run's, by name): through every call
    /// around it, all of them where a call inherits, only the named ones where it maps.
    pub(super) async fn call_secrets(
        &self,
        run: &Run,
        template: &JobTemplate,
        github: &Map<String, Value>,
        vars: &Map<String, Value>,
        run_inputs: Map<String, Value>,
        secrets: BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, ServiceError> {
        if template.calls.is_empty() {
            return Ok(secrets);
        }
        let jobs = self.run_jobs_with_outputs(run).await?;
        let (mut current, mut outer) = (secrets, run_inputs);
        for level in &template.calls {
            if let PassedSecrets::Map(map) = &level.secrets {
                let visible: Map<String, Value> =
                    current.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
                let contexts = level_contexts(level, github, vars, &outer, &jobs).with("secrets", visible);
                current = map
                    .iter()
                    .filter_map(|(name, value)| {
                        let rendered = Template::parse(value).and_then(|template| template.render(&contexts)).ok()?;
                        (!rendered.is_empty()).then(|| (name.clone(), rendered))
                    })
                    .collect();
            }
            let mut inputs = level.inputs.clone();
            let contexts = level_contexts(level, github, vars, &outer, &jobs);
            for (name, deferred) in &level.deferred {
                if let Ok(rendered) = Template::parse(&deferred.value).and_then(|template| template.render(&contexts)) {
                    inputs.insert(name.clone(), Value::String(rendered));
                }
            }
            outer = inputs;
        }
        Ok(current)
    }

    /// The calls' own `if:`, outermost first: `Some(false)` when one of them says no,
    /// `Err` when one cannot be evaluated.
    pub(super) async fn call_conditions(
        &self,
        run: &Run,
        template: &JobTemplate,
        github: &Map<String, Value>,
        vars: &Map<String, Value>,
        run_inputs: Map<String, Value>,
    ) -> Result<Result<bool, String>, ServiceError> {
        if template.calls.iter().all(|level| level.condition.is_none()) {
            return Ok(Ok(true));
        }
        let jobs = self.run_jobs_with_outputs(run).await?;
        let mut outer = run_inputs;
        for level in &template.calls {
            if let Some(condition) = &level.condition {
                let contexts = level_contexts(level, github, vars, &outer, &jobs);
                let status = ExprStatus::of_needs(level.needs.values().map(|source| need(source, &jobs, github, vars).result));
                let outcome = rbit_workflow::expr::Condition::parse(condition)
                    .and_then(|condition| condition.evaluate(&contexts.with_status(status)));
                match outcome {
                    Ok(true) => {}
                    Ok(false) => return Ok(Ok(false)),
                    Err(error) => return Ok(Err(format!("`{}`: `if`: {error}", level.name))),
                }
            }
            outer.clone_from(&level.inputs);
        }
        Ok(Ok(true))
    }

    /// Every job of `run` by key, with its status and outputs, instances together.
    pub(super) async fn run_jobs_with_outputs(&self, run: &Run) -> Result<RunJobs, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let mut by_key: RunJobs = BTreeMap::new();
        for job in store.jobs(run.id).await? {
            let outputs = store.outputs(job.id).await?.into_iter().collect();
            by_key.entry(job.key.clone()).or_default().push((job.status, outputs));
        }
        Ok(by_key)
    }

    // Actions: reusable workflows.
    /// Who may call `repo`'s workflows while it is private, for an administrator.
    pub async fn repo_access_level(&self, repo: &RepoContext) -> Result<String, ServiceError> {
        require_admin(repo)?;
        Ok(self.ctx().database.actions_settings().access_level(repo.repo.repo.id).await?)
    }

    /// Changes who may call `repo`'s workflows while it is private (audited).
    pub async fn set_repo_access_level(&self, actor: &User, repo: &RepoContext, level: &str) -> Result<(), ServiceError> {
        require_admin(repo)?;
        if !ACCESS_LEVELS.contains(&level) {
            return Err(ServiceError::Invalid(format!("access_level must be one of {}", ACCESS_LEVELS.join(", "))));
        }
        let store = self.ctx().database.actions_settings();
        if store.access_level(repo.repo.repo.id).await? == level {
            return Ok(());
        }
        store.set_access_level(repo.repo.repo.id, level, Timestamp::now()).await?;
        let detail = format!("workflows of {} may be called by: {level}", repo.repo.full_name());
        self.audit()
            .record(AuditEntry::new(AuditAction::ActionsPermissionsChanged).by(actor).on(repo.repo.full_name()).detail(detail))
            .await;
        Ok(())
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() {
        Ok(())
    } else if repo.access.can_read() {
        Err(ServiceError::Forbidden)
    } else {
        Err(ServiceError::NotFound)
    }
}

/// Every job of a run by key: the status and outputs of each instance (matrix
/// combinations, a called workflow's copies).
pub(super) type RunJobs = BTreeMap<String, Vec<(Status, BTreeMap<String, String>)>>;

/// The contexts of a calling job's `if`, `with` and `secrets`: the run's `github` and
/// `vars`, the inputs of the workflow it is in, its matrix, strategy and needs.
fn level_contexts(
    level: &CallLevel,
    github: &Map<String, Value>,
    vars: &Map<String, Value>,
    inputs: &Map<String, Value>,
    jobs: &RunJobs,
) -> ContextMap {
    let needs: Map<String, Value> = level
        .needs
        .iter()
        .map(|(name, source)| {
            let needed = need(source, jobs, github, vars);
            (name.clone(), json!({ "result": result_name(needed.result), "outputs": needed.outputs }))
        })
        .collect();
    let strategy = level.strategy;
    ContextMap::new()
        .with("github", github.clone())
        .with("vars", vars.clone())
        .with("inputs", inputs.clone())
        .with("matrix", level.matrix.clone())
        .with(
            "strategy",
            json!({
                "fail-fast": strategy.fail_fast,
                "job-index": strategy.job_index,
                "job-total": strategy.job_total,
                "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
            }),
        )
        .with("needs", needs)
}

/// What `source` gives a `needs` context: the result of its jobs and their outputs, or for
/// a job that called a workflow, the called workflow's `outputs` over its jobs.
pub(super) fn need(source: &NeedSource, jobs: &RunJobs, github: &Map<String, Value>, vars: &Map<String, Value>) -> Needed {
    let instances: Vec<&(Status, BTreeMap<String, String>)> = source.keys().iter().filter_map(|key| jobs.get(key)).flatten().collect();
    // As for a matrix: a failed job fails the whole, even when fail-fast cancelled others.
    let result = if instances.iter().any(|(status, _)| *status == Status::Failure) {
        JobResult::Failure
    } else {
        match rbit_core::actions::aggregate(instances.iter().map(|(status, _)| *status)) {
            Status::Success => JobResult::Success,
            Status::Skipped => JobResult::Skipped,
            Status::Cancelled => JobResult::Cancelled,
            _ => JobResult::Failure,
        }
    };
    let outputs = match source {
        NeedSource::Jobs(_) => {
            let mut outputs = BTreeMap::new();
            for (_, values) in &instances {
                for (key, value) in values {
                    outputs.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            outputs
        }
        NeedSource::Call { outputs, jobs: called, .. } => {
            let context: Map<String, Value> = called
                .iter()
                .map(|(id, inner)| {
                    let needed = need(inner, jobs, github, vars);
                    (id.clone(), json!({ "result": result_name(needed.result), "outputs": needed.outputs }))
                })
                .collect();
            let contexts = ContextMap::new().with("jobs", context).with("github", github.clone()).with("vars", vars.clone());
            outputs
                .iter()
                .map(|(name, value)| {
                    let rendered = Template::parse(value).and_then(|template| template.evaluate(&contexts)).map(|value| to_text(&value));
                    (name.clone(), rendered.unwrap_or_default())
                })
                .collect()
        }
    };
    Needed { result, outputs }
}

const fn result_name(result: JobResult) -> &'static str {
    match result {
        JobResult::Success => "success",
        JobResult::Failure => "failure",
        JobResult::Cancelled => "cancelled",
        JobResult::Skipped => "skipped",
    }
}

/// Whether `job` waits for the server to decide calls' `if:`.
pub fn decided_by_server(template: &JobTemplate) -> bool {
    template.deferred || template.condition.is_some() || template.calls.iter().any(|level| level.condition.is_some())
}
