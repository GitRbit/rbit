//! Run display names and run-scoped concurrency expression evaluation.
use super::contexts::RunContext;
use crate::error::WorkflowError;
use crate::expr::{Template, to_text};
use crate::model::Workflow;
use serde_json::Value;

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
    // `cancel-in-progress: ${{ github.event_name == 'pull_request' }}`, as rbit's own CI has.
    let cancel = Template::parse(&concurrency.cancel_in_progress)
        .and_then(|template| template.evaluate(&context.contexts()))
        .map_err(|problem| refuse(&problem))?;
    let cancel = match cancel {
        Value::Bool(value) => value,
        Value::String(text) if text.eq_ignore_ascii_case("true") => true,
        Value::String(text) if text.eq_ignore_ascii_case("false") => false,
        other => return Err(refuse(&format!("cancel-in-progress must be true or false, not `{}`", to_text(&other)))),
    };
    Ok(Some((group, cancel)))
}
