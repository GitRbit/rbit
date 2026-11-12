//! Does an event start a workflow? Forgejo's `detectMatched` rules with GitHub's pattern
//! dialect (see [`crate::pattern`]):
//!
//! * `push` — every filter given must pass; when both a branch and a tag filter are
//!   given, the one that applies to the ref decides; a branch filter alone never matches a
//!   tag (and the reverse); `paths` filters are skipped for tags; deletions, and refs that
//!   are neither branches nor tags, never match.
//! * `pull_request` — the activity type must be listed (`opened`, `reopened` and
//!   `synchronize` by default); `branches` match the base branch, `paths` the pull request's
//!   changed files.
//! * `workflow_dispatch` and `schedule` — the workflow declares them.
//!
//! Changed paths are asked for only when a `paths` filter needs them (see
//! [`ChangedPaths`]); over the caller's cap, `paths` filters count as passed, as on GitHub.
//! Dispatch inputs are checked against the workflow's `inputs` by [`resolve_inputs`].

use std::collections::BTreeMap;

use crate::model::{ActivityType, Filter, InputKind, PushFilters, Workflow, WorkflowRunType};

/// Markers in a head commit message (push) or a pull request title that skip its runs.
pub const SKIP_MARKERS: [&str; 5] = ["[skip ci]", "[ci skip]", "[no ci]", "[skip actions]", "[actions skip]"];
/// Most inputs one dispatch may set: as many as a workflow may declare, since the dispatch
/// form sends every one.
pub const MAX_DISPATCH_INPUTS_SET: usize = crate::parse::MAX_DISPATCH_INPUTS;
/// Longest dispatch input value, in bytes.
pub const MAX_DISPATCH_VALUE_BYTES: usize = 1024;

/// Whether `text` asks for its runs to be skipped.
pub fn has_skip_marker(text: &str) -> bool {
    SKIP_MARKERS.iter().any(|marker| text.contains(marker))
}

/// An event that might start a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// One updated ref of a push.
    Push {
        /// The full ref name: `refs/heads/main`, `refs/tags/v1.0`.
        reference: &'a str,
        /// The ref was deleted.
        deleted: bool,
    },
    PullRequest {
        action: &'a ActivityType,
        /// The base branch's short name.
        base_branch: &'a str,
    },
    Dispatch,
    /// A tick of one of the workflow's `schedule` entries, by its expression as written.
    Schedule {
        cron: &'a str,
    },
    // Actions: workflow_run and repository_dispatch.
    /// A run of the repository was requested, started or completed.
    WorkflowRun {
        /// The triggering workflow's name.
        workflow: &'a str,
        action: WorkflowRunType,
        /// The triggering run's branch, or tag; `None` for neither.
        branch: Option<&'a str>,
    },
    RepositoryDispatch {
        event_type: &'a str,
    },
}

/// The paths an event changed, computed only when a filter needs them.
pub trait ChangedPaths {
    /// The changed paths, or `None` when there are too many to list (then `paths` filters
    /// pass).
    fn changed_paths(&mut self) -> Option<&[String]>;
}

/// [`ChangedPaths`] computed once, on first use, by a closure.
#[derive(Debug)]
pub struct LazyPaths<F> {
    /// Taken on first use.
    compute: Option<F>,
    /// `None` before the first use, and when there were too many paths.
    paths: Option<Vec<String>>,
}

impl<F: FnOnce() -> Option<Vec<String>>> LazyPaths<F> {
    pub fn new(compute: F) -> Self {
        Self { compute: Some(compute), paths: None }
    }

    /// Whether the paths were computed.
    pub fn was_computed(&self) -> bool {
        self.compute.is_none()
    }
}

impl<F: FnOnce() -> Option<Vec<String>>> ChangedPaths for LazyPaths<F> {
    fn changed_paths(&mut self) -> Option<&[String]> {
        if let Some(compute) = self.compute.take() {
            self.paths = compute();
        }
        self.paths.as_deref()
    }
}

/// A fixed list of changed paths (`None`: too many).
impl ChangedPaths for Option<Vec<String>> {
    fn changed_paths(&mut self) -> Option<&[String]> {
        self.as_deref()
    }
}

/// Whether `event` starts `workflow`.
pub fn matches(workflow: &Workflow, event: &Event<'_>, changed: &mut dyn ChangedPaths) -> bool {
    match *event {
        Event::Push { reference, deleted } => {
            !deleted && workflow.on.push.as_ref().is_some_and(|filters| push_matches(filters, reference, changed))
        }
        Event::PullRequest { action, base_branch } => workflow.on.pull_request.as_ref().is_some_and(|filters| {
            let listed = match &filters.types {
                Some(types) => types.contains(action),
                None => ActivityType::DEFAULT.contains(action),
            };
            listed && filters.branches.passes([base_branch]) && paths_pass(&filters.paths, changed)
        }),
        Event::Dispatch => workflow.on.workflow_dispatch.is_some(),
        Event::Schedule { cron } => workflow.on.schedule.iter().any(|entry| entry.cron.as_str() == cron),
        Event::WorkflowRun { workflow: name, action, branch } => workflow.on.workflow_run.as_ref().is_some_and(|filters| {
            filters.workflows.iter().any(|listed| listed == name)
                && filters.types.as_ref().is_none_or(|types| types.contains(&action))
                && (filters.branches.is_absent() || branch.is_some_and(|branch| filters.branches.passes([branch])))
        }),
        Event::RepositoryDispatch { event_type } => workflow
            .on
            .repository_dispatch
            .as_ref()
            .is_some_and(|filters| filters.types.as_ref().is_none_or(|types| types.iter().any(|listed| listed == event_type))),
    }
}

fn push_matches(filters: &PushFilters, reference: &str, changed: &mut dyn ChangedPaths) -> bool {
    // A scratch ref (`refs/scratch/<owner>/<name>`, a commit on no branch pushed for CI) and a
    // merge-queue candidate (`refs/merge-queue/<base>/pr-<n>`) are pushed like branches named so.
    let branch = reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/").filter(|rest| rest.starts_with("scratch/") || rest.starts_with("merge-queue/")));
    let tag = reference.strip_prefix("refs/tags/");
    let ref_filter = match (filters.branches.is_absent(), filters.tags.is_absent()) {
        (true, true) => branch.is_some() || tag.is_some(),
        // Only branch filters: a tag never matches (Forgejo's `hasBranchFilter`), and the
        // reverse.
        (false, true) => branch.is_some_and(|name| filters.branches.passes([name])),
        (true, false) => tag.is_some_and(|name| filters.tags.passes([name])),
        // Both: the filter for the ref's kind decides.
        (false, false) => match (branch, tag) {
            (Some(name), _) => filters.branches.passes([name]),
            (_, Some(name)) => filters.tags.passes([name]),
            _ => false,
        },
    };
    // Tags have no changed paths; their pushes skip `paths` filters.
    ref_filter && (tag.is_some() || paths_pass(&filters.paths, changed))
}

fn paths_pass(filter: &Filter, changed: &mut dyn ChangedPaths) -> bool {
    if filter.is_absent() {
        return true;
    }
    changed.changed_paths().is_none_or(|paths| filter.passes(paths))
}

/// A checked `workflow_dispatch` input value.
#[derive(Debug, Clone, PartialEq)]
pub enum InputValue {
    String(String),
    Boolean(bool),
    Number(f64),
}

impl InputValue {
    /// As GitHub puts it in `github.event.inputs`: always a string.
    pub fn to_text(&self) -> String {
        match self {
            Self::String(text) => text.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) => crate::yaml::format_float(*value),
        }
    }
}

/// Checks the inputs given to a dispatch against the workflow's `inputs` and fills in the
/// defaults. Every message is safe to show to the person dispatching.
pub fn resolve_inputs(workflow: &Workflow, given: &BTreeMap<String, String>) -> Result<BTreeMap<String, InputValue>, String> {
    let Some(dispatch) = &workflow.on.workflow_dispatch else {
        return Err("this workflow does not run on workflow_dispatch".into());
    };
    if given.len() > MAX_DISPATCH_INPUTS_SET {
        return Err(format!("at most {MAX_DISPATCH_INPUTS_SET} inputs can be set"));
    }
    if let Some(unknown) = given.keys().find(|name| !dispatch.inputs.iter().any(|input| input.name == **name)) {
        return Err(format!("the workflow has no input `{unknown}`"));
    }
    let mut resolved = BTreeMap::new();
    for input in &dispatch.inputs {
        let value = match given.get(&input.name).filter(|value| !value.is_empty()).or(input.default.as_ref()) {
            Some(value) => value,
            None if input.required => return Err(format!("the input `{}` is required", input.name)),
            None if input.kind == InputKind::Boolean => {
                resolved.insert(input.name.clone(), InputValue::Boolean(false));
                continue;
            }
            None => {
                resolved.insert(input.name.clone(), InputValue::String(String::new()));
                continue;
            }
        };
        if value.len() > MAX_DISPATCH_VALUE_BYTES {
            return Err(format!("the input `{}` must be at most {MAX_DISPATCH_VALUE_BYTES} bytes", input.name));
        }
        let checked = match &input.kind {
            InputKind::String | InputKind::Environment => InputValue::String(value.clone()),
            InputKind::Boolean => match value.as_str() {
                "true" => InputValue::Boolean(true),
                "false" => InputValue::Boolean(false),
                _ => return Err(format!("the input `{}` must be true or false", input.name)),
            },
            InputKind::Number => match value.trim().parse::<f64>() {
                Ok(number) if number.is_finite() => InputValue::Number(number),
                _ => return Err(format!("the input `{}` must be a number", input.name)),
            },
            InputKind::Choice(options) => {
                if !options.contains(value) {
                    return Err(format!("the input `{}` must be one of: {}", input.name, options.join(", ")));
                }
                InputValue::String(value.clone())
            }
        };
        resolved.insert(input.name.clone(), checked);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests;
