//! A workflow planned for one event before its run exists: its jobs and its run-level
//! concurrency group, with groups that only one run could ever have dropped.
use super::group_context::GroupContext;
use super::loader::Loader;
use super::names::concurrency_group;
use super::planning::plan_with;
use crate::error::WorkflowError;
use crate::limits::{Deadline, Limits};
use crate::model::Workflow;
use crate::spec::JobTemplate;
use serde_json::Value;

/// A workflow planned for one event: its jobs and its concurrency group.
#[derive(Debug, Clone)]
pub struct RunPlan {
    pub jobs: Vec<JobTemplate>,
    /// The evaluated `concurrency.group` and `cancel-in-progress`.
    pub concurrency: Option<(String, bool)>,
}

/// Plans `workflow` (named `name`, at `path`) for an event described by `group`.
///
/// A group naming `github.run_id` or `github.run_number` is unique to its run: it never
/// waits for or cancels another, so it is no group at all. Neither is known before the run
/// exists, so planning again with other values tells; such run and job groups are dropped.
pub fn plan_run(
    workflow: &Workflow,
    name: &str,
    path: &str,
    group: &GroupContext,
    limits: &Limits,
    deadline: &Deadline,
    loader: &mut dyn Loader,
) -> Result<RunPlan, WorkflowError> {
    let context = GroupContext { workflow: name.to_owned(), workflow_path: path.to_owned(), run_id: 0, ..group.clone() };
    let mut jobs = plan_with(workflow, &context.run_context(), limits, deadline, loader)?;
    let other_run = {
        let mut other = GroupContext { run_id: 1, ..context.clone() }.run_context();
        other.github.insert("run_number".to_owned(), Value::String("1".to_owned()));
        other
    };
    let concurrency = match concurrency_group(workflow, &context.run_context())? {
        Some((group, cancel)) => match concurrency_group(workflow, &other_run)? {
            Some((other, _)) if other == group => Some((group, cancel)),
            _ => None,
        },
        None => None,
    };
    if jobs.iter().any(|job| job.concurrency.is_some())
        && let Ok(others) = plan_with(workflow, &other_run, limits, deadline, loader)
    {
        for (job, other) in jobs.iter_mut().zip(&others) {
            let group = job.concurrency.as_ref().map(|concurrency| &concurrency.group);
            if job.job_id == other.job_id && group != other.concurrency.as_ref().map(|concurrency| &concurrency.group) {
                job.concurrency = None;
            }
        }
    }
    Ok(RunPlan { jobs, concurrency })
}
