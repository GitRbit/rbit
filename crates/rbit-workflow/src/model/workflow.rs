//! Typed workflow workflow records; expression text remains unevaluated.
use super::calls::WorkflowCall;
use super::events::{Dispatch, PullRequestFilters, PushFilters, RepositoryDispatchFilters, Schedule, WorkflowRunFilters};
use super::jobs::{Concurrency, Job, Permissions};
use crate::spec::{Env, RunDefaults};

/// A parsed workflow file.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// `name`; runs show the file path when it is absent.
    pub name: Option<String>,
    /// `run-name`, as written (it may contain expressions).
    pub run_name: Option<String>,
    pub on: On,
    /// Top-level `env`, as templates.
    pub env: Env,
    /// Top-level `defaults.run`.
    pub defaults: RunDefaults,
    /// Top-level `permissions`.
    pub permissions: Option<Permissions>,
    /// Workflow-level `concurrency`.
    pub concurrency: Option<Concurrency>,
    /// The jobs in file order.
    pub jobs: Vec<Job>,
}

impl Workflow {
    pub fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }
}

/// The events a workflow runs on (`on:`).
#[derive(Debug, Clone, Default)]
pub struct On {
    pub push: Option<PushFilters>,
    pub pull_request: Option<PullRequestFilters>,
    pub workflow_dispatch: Option<Dispatch>,
    /// `schedule` entries, in file order.
    pub schedule: Vec<Schedule>,
    // Actions: reusable workflows.
    /// `workflow_call`: other workflows may call this one.
    pub workflow_call: Option<WorkflowCall>,
    // Actions: workflow_run and repository_dispatch.
    pub workflow_run: Option<WorkflowRunFilters>,
    pub repository_dispatch: Option<RepositoryDispatchFilters>,
    /// Other events, by name. rbit starts no runs for them.
    pub other: Vec<String>,
}
