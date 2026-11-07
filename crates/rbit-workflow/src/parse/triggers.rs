//! Event forms, activity/branch/path filters and schedule validation.
use super::calls::parse_workflow_call;
use super::dispatch::parse_dispatch;
use super::syntax::{error, mapping, strings};
use crate::cron::{Cron, MIN_INTERVAL_MINUTES};
use crate::error::{Location, WorkflowError};
use crate::model::{
    ActivityType, Filter, On, PullRequestFilters, PushFilters, RepositoryDispatchFilters, Schedule, WorkflowRunFilters, WorkflowRunType,
};
use crate::pattern::Patterns;
use crate::yaml::{Mapping, Node, Value};

/// Every event GitHub documents for `on`. rbit starts runs for some of them
/// ([`crate::trigger`]); the others are accepted and never fire.
pub(super) const GITHUB_EVENTS: [&str; 35] = [
    "branch_protection_rule",
    "check_run",
    "check_suite",
    "create",
    "delete",
    "deployment",
    "deployment_status",
    "discussion",
    "discussion_comment",
    "fork",
    "gollum",
    "issue_comment",
    "issues",
    "label",
    "merge_group",
    "milestone",
    "page_build",
    "project",
    "project_card",
    "project_column",
    "public",
    "pull_request",
    "pull_request_review",
    "pull_request_review_comment",
    "pull_request_target",
    "push",
    "registry_package",
    "release",
    "repository_dispatch",
    "schedule",
    "status",
    "watch",
    "workflow_call",
    "workflow_dispatch",
    "workflow_run",
];

pub(super) fn parse_on(node: &Node) -> Result<On, WorkflowError> {
    let mut on = On::default();
    match &node.value {
        Value::Scalar(_) | Value::Sequence(_) => {
            for event in strings(node, "on")? {
                add_event(&mut on, &event, None, node.at)?;
            }
        }
        Value::Mapping(events) => {
            for (key, value) in events.iter() {
                add_event(&mut on, &key.name, Some(value), key.at)?;
            }
        }
    }
    Ok(on)
}

pub(super) fn add_event(on: &mut On, event: &str, config: Option<&Node>, at: Location) -> Result<(), WorkflowError> {
    let config = config.filter(|node| !node.is_null());
    match event {
        "push" => on.push = Some(config.map(parse_push).transpose()?.unwrap_or_default()),
        "pull_request" => on.pull_request = Some(config.map(parse_pull_request).transpose()?.unwrap_or_default()),
        "workflow_dispatch" => on.workflow_dispatch = Some(config.map(parse_dispatch).transpose()?.unwrap_or_default()),
        "schedule" => match config {
            Some(node) => on.schedule = parse_schedule(node)?,
            None => return Err(error("`on.schedule` needs a list of `cron` entries", at)),
        },
        // Actions: reusable workflows.
        "workflow_call" => on.workflow_call = Some(config.map(parse_workflow_call).transpose()?.unwrap_or_default()),
        // Actions: workflow_run and repository_dispatch.
        "workflow_run" => match config {
            Some(node) => on.workflow_run = Some(parse_workflow_run(node)?),
            None => return Err(error("`on.workflow_run` needs `workflows`", at)),
        },
        "repository_dispatch" => on.repository_dispatch = Some(config.map(parse_repository_dispatch).transpose()?.unwrap_or_default()),
        other if GITHUB_EVENTS.contains(&other) => on.other.push(other.to_owned()),
        other => return Err(error(format!("`on.{other}` is not an event"), at)),
    }
    Ok(())
}

/// Reads the filter pair `name` / `name-ignore` of an event.
pub(super) fn filter(filters: &Mapping, event: &str, name: &str) -> Result<Filter, WorkflowError> {
    let ignore = format!("{name}-ignore");
    match (filters.entry(name), filters.entry(&ignore)) {
        (Some((key, _)), Some(_)) => Err(error(format!("`on.{event}` cannot have both `{name}` and `{ignore}`"), key.at)),
        (Some((_, node)), None) => Ok(Filter::Include(patterns(node, &format!("on.{event}.{name}"))?)),
        (None, Some((_, node))) => Ok(Filter::Ignore(patterns(node, &format!("on.{event}.{ignore}"))?)),
        (None, None) => Ok(Filter::Absent),
    }
}

pub(super) fn patterns(node: &Node, path: &str) -> Result<Patterns, WorkflowError> {
    let values = strings(node, path)?;
    Patterns::compile(&values).map_err(|problem| error(format!("`{path}`: {problem}"), node.at))
}

pub(super) fn only_keys(filters: &Mapping, event: &str, allowed: &[&str]) -> Result<(), WorkflowError> {
    match filters.iter().find(|(key, _)| !allowed.contains(&key.name.as_str())) {
        Some((key, _)) => {
            Err(error(format!("`on.{event}` does not support `{}` (rbit supports {})", key.name, allowed.join(", ")), key.at))
        }
        None => Ok(()),
    }
}

pub(super) fn parse_push(node: &Node) -> Result<PushFilters, WorkflowError> {
    let filters = mapping(node, "on.push")?;
    only_keys(filters, "push", &["branches", "branches-ignore", "tags", "tags-ignore", "paths", "paths-ignore"])?;
    Ok(PushFilters {
        branches: filter(filters, "push", "branches")?,
        tags: filter(filters, "push", "tags")?,
        paths: filter(filters, "push", "paths")?,
    })
}

pub(super) fn parse_pull_request(node: &Node) -> Result<PullRequestFilters, WorkflowError> {
    let filters = mapping(node, "on.pull_request")?;
    only_keys(filters, "pull_request", &["types", "branches", "branches-ignore", "paths", "paths-ignore"])?;
    let types = match filters.get("types") {
        None => None,
        Some(node) => Some(
            strings(node, "on.pull_request.types")?
                .iter()
                .map(|name| {
                    ActivityType::parse(name)
                        .ok_or_else(|| error(format!("`on.pull_request.types`: `{name}` is not a pull request activity type"), node.at))
                })
                .collect::<Result<_, _>>()?,
        ),
    };
    Ok(PullRequestFilters {
        types,
        branches: filter(filters, "pull_request", "branches")?,
        paths: filter(filters, "pull_request", "paths")?,
    })
}

// Actions: workflow_run and repository_dispatch.
pub(super) fn parse_workflow_run(node: &Node) -> Result<WorkflowRunFilters, WorkflowError> {
    let filters = mapping(node, "on.workflow_run")?;
    only_keys(filters, "workflow_run", &["workflows", "types", "branches", "branches-ignore"])?;
    let Some(workflows) = filters.get("workflows") else {
        return Err(error("`on.workflow_run` needs `workflows`", node.at));
    };
    let workflows = strings(workflows, "on.workflow_run.workflows")?;
    if workflows.is_empty() || workflows.iter().any(String::is_empty) {
        return Err(error("`on.workflow_run.workflows` names one or more workflows", node.at));
    }
    let types = match filters.get("types") {
        None => None,
        Some(node) => Some(
            strings(node, "on.workflow_run.types")?
                .iter()
                .map(|name| {
                    WorkflowRunType::parse(name).ok_or_else(|| {
                        error(format!("`on.workflow_run.types`: `{name}` is not requested, in_progress or completed"), node.at)
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
    };
    Ok(WorkflowRunFilters { workflows, types, branches: filter(filters, "workflow_run", "branches")? })
}

pub(super) fn parse_repository_dispatch(node: &Node) -> Result<RepositoryDispatchFilters, WorkflowError> {
    let filters = mapping(node, "on.repository_dispatch")?;
    only_keys(filters, "repository_dispatch", &["types"])?;
    let types = filters.get("types").map(|node| strings(node, "on.repository_dispatch.types")).transpose()?;
    Ok(RepositoryDispatchFilters { types })
}

pub(super) fn parse_schedule(node: &Node) -> Result<Vec<Schedule>, WorkflowError> {
    let entries = node.as_sequence().ok_or_else(|| error("`on.schedule` must be a list of `cron` entries", node.at))?;
    entries
        .iter()
        .map(|entry| {
            let (cron, at) = mapping(entry, "on.schedule[]")?
                .get("cron")
                .and_then(|node| Some((node.as_text()?, node.at)))
                .ok_or_else(|| error("each `on.schedule` entry needs a `cron` string", entry.at))?;
            let cron = Cron::parse(&cron).map_err(|problem| error(format!("`on.schedule`: {problem}"), at))?;
            if cron.is_too_frequent() {
                return Err(error(format!("`on.schedule`: `{cron}` runs more often than every {MIN_INTERVAL_MINUTES} minutes"), at));
            }
            Ok(Schedule { cron, at: entry.at })
        })
        .collect()
}
