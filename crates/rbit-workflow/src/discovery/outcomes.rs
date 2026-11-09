//! What each discovered workflow makes of an event.

use super::WorkflowFile;
use crate::error::WorkflowError;
use crate::limits::{Deadline, Limits};
use crate::plan::{GroupContext, Loader, RunPlan, plan_run};
use crate::trigger::{ChangedPaths, Event, matches};

/// One workflow file's answer to an event.
#[derive(Debug, Clone)]
pub struct Evaluated {
    /// `.forgejo/workflows/ci.yml`.
    pub path: String,
    /// The workflow's `name`, or its path ([`WorkflowFile::name`]).
    pub name: String,
    /// The run's plan, or why the file has none: it does not parse, or cannot be planned.
    pub outcome: Result<RunPlan, WorkflowError>,
}

/// The workflows of `files` that `event` starts, planned with `group`, and every file that
/// does not parse, in file order. Workflows the event does not start are left out.
pub fn evaluate(
    files: &[WorkflowFile],
    event: &Event<'_>,
    changed: &mut dyn ChangedPaths,
    group: &GroupContext,
    limits: &Limits,
    deadline: &Deadline,
    loader: &mut dyn Loader,
) -> Vec<Evaluated> {
    let mut evaluated = Vec::new();
    for file in files {
        let outcome = match &file.parsed {
            Err(error) => Err(error.clone()),
            Ok(workflow) if matches(workflow, event, changed) => {
                plan_run(workflow, &file.name(), &file.path, group, limits, deadline, loader)
            }
            Ok(_) => continue,
        };
        evaluated.push(Evaluated { path: file.path.clone(), name: file.name(), outcome });
    }
    evaluated
}
