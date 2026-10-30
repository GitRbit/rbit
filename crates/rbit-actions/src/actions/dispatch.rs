//! `workflow_dispatch` from the UI and the API: the ref must be a branch or tag of the
//! repository itself, and inputs are checked against the workflow's `inputs`.
//!
//! * Only writers dispatch. The caller is a [`User`]: task tokens are no user, so a
//!   workflow can never dispatch another (§11.5); the API refuses task callers before it
//!   gets here.
//! * The ref is one of the repository's own `refs/heads/*` or `refs/tags/*` (§11.7), given
//!   in full or by its short name (a branch wins over a tag of the same name). Never
//!   `refs/pull/*` and never a commit id: a fork's pull request head is in the repository,
//!   but it is not the repository's code.
//! * The workflow is read at that ref, from the same directory as for every other event,
//!   by its file name or path; inputs follow `rbit_workflow::trigger::resolve_inputs` (at
//!   most 10 set, values at most 1 KiB, `choice`, `boolean` and `number` checked). Problems
//!   are answered to the person dispatching; no workflow-error run is recorded.
//! * The pages offer a form ([`ActionsService::dispatch_form`]) on a workflow's page: the
//!   workflow's inputs at a branch or tag, read as a dispatch reads them. The workflows
//!   on the default branch ([`ActionsService::workflow_paths`]) get a page even before they
//!   ever ran, so a workflow that only runs on `workflow_dispatch` can be started at all.
//!
//! Workstream T (triggers and runs) owns this file.

use std::{collections::BTreeMap, path::Path};

use rbit_core::{
    User,
    actions::{Run, RunEvent, RunTrust},
};
use rbit_git::{EntryKind, GitError, RefKind};
use rbit_workflow::{Event, GroupContext, model::DispatchInput, trigger::resolve_inputs};

use super::{
    ActionsService, context,
    runs::{Candidate, Trigger},
    triggers::{MAX_WORKFLOW_FILES, WORKFLOW_DIRS, describe, discover_one, plan_workflow},
};
use crate::{RepoContext, ServiceError};

/// A dispatch as submitted.
#[derive(Debug, Clone, Default)]
pub struct DispatchRequest {
    /// The workflow's file name (`ci.yml`) or its path (`.forgejo/workflows/ci.yml`).
    pub workflow: String,
    /// A branch or tag: `main`, `refs/heads/main`, `v1.0`, `refs/tags/v1.0`.
    pub reference: String,
    /// Input values by name, as typed.
    pub inputs: BTreeMap<String, String>,
}

// Actions: dispatch form.
/// What the dispatch form of one workflow asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchForm {
    /// `.forgejo/workflows/release.yml`.
    pub path: String,
    /// The workflow's `name`, or its path.
    pub name: String,
    /// The full ref the workflow was read at: `refs/heads/main`.
    pub reference: String,
    pub inputs: Vec<DispatchInput>,
}

impl ActionsService {
    // Actions: dispatch form.
    /// The workflow files on `repo`'s default branch, by path, from the first of
    /// [`WORKFLOW_DIRS`] that exists, as runs read them: what the pages list beside the
    /// workflows that ran. Nothing is parsed. Empty for an empty repository.
    pub async fn workflow_paths(&self, repo: &RepoContext) -> Result<Vec<String>, ServiceError> {
        self.require_pages(repo).await?;
        let branch = format!("refs/heads/{}", repo.repo.repo.default_branch);
        let sha = match repo.reader.commit_id(branch).await {
            Ok(sha) => sha,
            Err(GitError::NotFound(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        for dir in WORKFLOW_DIRS {
            let tree = match repo.reader.tree(sha.clone(), dir.to_owned()).await {
                Ok(tree) => tree,
                Err(GitError::NotFound(_)) => continue,
                Err(error) => return Err(error.into()),
            };
            let mut paths: Vec<String> = tree
                .entries
                .into_iter()
                .filter(|entry| matches!(entry.kind, EntryKind::File | EntryKind::Executable))
                .filter(|entry| Path::new(&entry.name).extension().is_some_and(|extension| extension == "yml" || extension == "yaml"))
                .map(|entry| format!("{dir}/{}", entry.name))
                .collect();
            paths.sort();
            paths.truncate(MAX_WORKFLOW_FILES);
            return Ok(paths);
        }
        Ok(Vec::new())
    }

    /// The form that dispatches `workflow` (its file name or path) at `reference`, a branch
    /// or tag of `repo` (its default branch when blank), for someone who may dispatch it:
    /// `None` when the file is not there or does not run on `workflow_dispatch`. A workflow
    /// that does not parse, or a ref that is no branch or tag, is `Invalid`.
    pub async fn dispatch_form(&self, repo: &RepoContext, workflow: &str, reference: &str) -> Result<Option<DispatchForm>, ServiceError> {
        if !repo.access.can_write() {
            return Err(if repo.access.can_read() { ServiceError::Forbidden } else { ServiceError::NotFound });
        }
        self.require_pages(repo).await?;
        let given = if reference.trim().is_empty() { repo.repo.repo.default_branch.as_str() } else { reference };
        let reference = own_ref(repo, given).await?;
        let sha = repo.reader.commit_id(reference.clone()).await?;
        let file = workflow_file(workflow)?;
        let (reader, limits) = (repo.reader.clone(), self.limits());
        let read = self
            .evaluate(move |handle, deadline| {
                // Only the one file: every writer's view of a workflow page reads it.
                let found = discover_one(handle, &reader, &sha, |path| file.matches(path), &limits, deadline).map_err(|error| {
                    tracing::warn!(%error, "cannot read the workflow for a dispatch form");
                    "the workflow could not be read".to_owned()
                })?;
                let Some(found) = found else { return Ok(None) };
                let name = found.name();
                let workflow = found.parsed.as_ref().map_err(|error| describe(&found.path, error))?;
                Ok(workflow.on.workflow_dispatch.as_ref().map(|dispatch| (found.path.clone(), name, dispatch.inputs.clone())))
            })
            .await?;
        let read = read.map_err(ServiceError::Invalid)?;
        Ok(read.map(|(path, name, inputs)| DispatchForm { path, name, reference, inputs }))
    }

    /// Starts `request`'s workflow for `actor`, who needs write access to `repo`.
    pub async fn dispatch(&self, actor: &User, repo: &RepoContext, request: &DispatchRequest) -> Result<Run, ServiceError> {
        if !repo.access.can_write() {
            return Err(if repo.access.can_read() { ServiceError::Forbidden } else { ServiceError::NotFound });
        }
        repo.ensure_writable()?;
        let owned = &repo.repo;
        if !self.ctx().database.actions_settings().is_enabled(owned.repo.id).await? {
            return Err(ServiceError::NotFound);
        }
        let reference = own_ref(repo, &request.reference).await?;
        let sha = repo.reader.commit_id(reference.clone()).await?;
        let file = workflow_file(&request.workflow)?;

        let reader = repo.reader.clone();
        let (limits, given) = (self.limits(), request.inputs.clone());
        // Dispatching needs write access, so the run is trusted.
        let group = self.planning(owned, RunEvent::WorkflowDispatch, &reference, &sha, (Some(actor.id()), Some(actor.id())), true).await?;
        let evaluated_sha = sha.clone();
        let (this, caller) = (self.clone(), owned.clone());
        let evaluated = self
            .evaluate(move |handle, deadline| {
                let found =
                    discover_one(handle, &reader, &evaluated_sha, |path| file.matches(path), &limits, deadline).map_err(|error| {
                        tracing::warn!(%error, "cannot read the workflow to dispatch");
                        "the workflow could not be read".to_owned()
                    })?;
                let Some(found) = found else {
                    return Err(format!("there is no workflow `{}` at this ref", file.shown()));
                };
                let workflow = found.parsed.as_ref().map_err(|error| describe(&found.path, error))?;
                if !rbit_workflow::matches(workflow, &Event::Dispatch, &mut None::<Vec<String>>) {
                    return Err(format!("{} does not run on workflow_dispatch", found.path));
                }
                let inputs = resolve_inputs(workflow, &given)?;
                let typed = match context::typed_inputs(&inputs) {
                    serde_json::Value::Object(typed) => typed,
                    _ => serde_json::Map::new(),
                };
                let group = GroupContext { inputs: typed, ..group };
                let mut loader = super::calls::CallLoader::new(handle, &this, &caller, &reader, &evaluated_sha, &limits, deadline);
                let plan = plan_workflow(workflow, &found.name(), &found.path, &group, &limits, deadline, &mut loader)
                    .map_err(|error| describe(&found.path, &error))?;
                Ok((Candidate { path: found.path.clone(), name: found.name(), outcome: Ok(plan) }, inputs))
            })
            .await?;
        let (candidate, inputs) = evaluated.map_err(ServiceError::Invalid)?;
        // Actions: disabled workflows.
        if self.ctx().database.actions_settings().is_workflow_disabled(owned.repo.id, &candidate.path).await? {
            return Err(ServiceError::Invalid(format!("{} is disabled; enable it to run it", candidate.path)));
        }

        let database = &self.ctx().database;
        let fork = database.forks().forks_among(&[owned.repo.id]).await?.contains(&owned.repo.id);
        let base_url = self.ctx().config.server.base_url();
        let payload = context::dispatch_event(owned, fork, &actor.account, &candidate.path, &reference, &inputs, base_url);
        let payload = context::fit_payload(payload);
        let typed = context::typed_inputs(&inputs).to_string();
        let title = format!("{} ran manually", candidate.name);
        let trigger = Trigger {
            repo: owned,
            event: RunEvent::WorkflowDispatch,
            reference: &reference,
            sha: &sha,
            title: &title,
            actor: Some(actor.id()),
            trigger_actor: Some(actor.id()),
            pull_number: None,
            is_fork: false,
            trust: RunTrust::Trusted,
            payload: &payload,
            inputs: Some(&typed),
        };
        let mut allowance = self.allowance();
        let run = self.start_run(&trigger, candidate, &mut allowance).await?.ok_or(ServiceError::Forbidden)?;
        tracing::info!(repo = %owned.full_name(), run = run.id.0, by = %actor.name(), "workflow dispatched");
        Ok(run)
    }
}

/// The full ref `given` names among the repository's own branches and tags.
async fn own_ref(repo: &RepoContext, given: &str) -> Result<String, ServiceError> {
    let refused = || ServiceError::Invalid("the ref must be a branch or tag of this repository".to_owned());
    let given = given.trim();
    let (kinds, name): (&[RefKind], &str) = if let Some(branch) = given.strip_prefix("refs/heads/") {
        (&[RefKind::Branch], branch)
    } else if let Some(tag) = given.strip_prefix("refs/tags/") {
        (&[RefKind::Tag], tag)
    } else if given.starts_with("refs/") || given.is_empty() {
        return Err(refused());
    } else {
        (&[RefKind::Branch, RefKind::Tag], given)
    };
    let refs = repo.reader.refs().await?;
    for kind in kinds {
        if refs.iter().any(|summary| summary.kind == *kind && summary.name == name) {
            let prefix = if *kind == RefKind::Branch { "refs/heads/" } else { "refs/tags/" };
            return Ok(format!("{prefix}{name}"));
        }
    }
    Err(refused())
}

/// The workflow a dispatch names: a file name, or a path inside a workflow directory.
#[derive(Debug, Clone)]
struct WorkflowFileName {
    /// The directory it was given with, if any.
    dir: Option<&'static str>,
    name: String,
}

impl WorkflowFileName {
    fn matches(&self, path: &str) -> bool {
        match self.dir {
            Some(dir) => path.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')) == Some(self.name.as_str()),
            None => path.rsplit('/').next() == Some(self.name.as_str()),
        }
    }

    fn shown(&self) -> String {
        match self.dir {
            Some(dir) => format!("{dir}/{}", self.name),
            None => self.name.clone(),
        }
    }
}

fn workflow_file(given: &str) -> Result<WorkflowFileName, ServiceError> {
    let given = given.trim();
    let (dir, name) =
        match WORKFLOW_DIRS.iter().find_map(|dir| given.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')).map(|rest| (*dir, rest)))
        {
            Some((dir, name)) => (Some(dir), name),
            None => (None, given),
        };
    let valid = !name.is_empty()
        && !name.contains(['/', '\\'])
        && name != "."
        && name != ".."
        && Path::new(name).extension().is_some_and(|extension| extension == "yml" || extension == "yaml");
    if !valid {
        return Err(ServiceError::Invalid("name a workflow file, such as ci.yml".to_owned()));
    }
    Ok(WorkflowFileName { dir, name: name.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_files_are_named_by_file_name_or_path() {
        let name = workflow_file("ci.yml").unwrap();
        assert!(name.matches(".forgejo/workflows/ci.yml") && name.matches(".github/workflows/ci.yml"));
        assert!(!name.matches(".forgejo/workflows/other.yml"));
        let path = workflow_file(".github/workflows/ci.yaml").unwrap();
        assert!(path.matches(".github/workflows/ci.yaml") && !path.matches(".forgejo/workflows/ci.yaml"));
        assert_eq!(path.shown(), ".github/workflows/ci.yaml");
        for refused in ["", "ci", "../ci.yml", "a/ci.yml", ".forgejo/workflows/", "x\\ci.yml", "README.md"] {
            assert!(workflow_file(refused).is_err(), "{refused}");
        }
    }
}
