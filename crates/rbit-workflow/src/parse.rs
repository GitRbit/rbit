//! Workflow file → [`Workflow`], with every problem reported at its place in the file.
//!
//! Checks here are structural: the shape of `on`, filters that compile, job ids, `runs-on`,
//! `needs` that name existing jobs without a cycle, a matrix that is a mapping of lists,
//! `permissions` and `concurrency`. Expressions are left as written; planning
//! evaluates the few rbit evaluates itself and the runner evaluates the rest.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    cron::{Cron, MIN_INTERVAL_MINUTES},
    error::{ErrorKind, Location, WorkflowError},
    limits::{Deadline, Limits},
    model::{
        ActivityType, Concurrency, Dispatch, DispatchInput, Filter, InputKind, Job, Matrix, On, Permission, Permissions,
        PullRequestFilters, PushFilters, Schedule, Workflow,
    },
    pattern::Patterns,
    yaml::{self, Key, Mapping, Node, ScalarValue, Value},
};

/// Most `workflow_dispatch` inputs.
pub const MAX_DISPATCH_INPUTS: usize = 25;
/// Longest job id, in bytes.
pub const MAX_JOB_ID_BYTES: usize = 100;

/// Parses and validates one workflow file.
pub fn parse(source: &[u8], limits: &Limits, deadline: &Deadline) -> Result<Workflow, WorkflowError> {
    if source.len() > limits.max_bytes {
        return Err(WorkflowError::limit(format!("the workflow file is larger than {} bytes", limits.max_bytes), None));
    }
    let source = std::str::from_utf8(source).map_err(|_| WorkflowError::new(ErrorKind::Parse, "the workflow file is not UTF-8"))?;
    let document = yaml::parse_document(source, limits)?;
    deadline.check()?;
    let Value::Mapping(document) = document.value else {
        return Err(WorkflowError::parse("a workflow must be a mapping with `on` and `jobs`", document.at));
    };
    let name = optional_text(&document, "name")?;
    let run_name = optional_text(&document, "run-name")?;
    let on = match document.entry("on") {
        Some((_, node)) => parse_on(node)?,
        None => return Err(WorkflowError::parse("the workflow has no `on` section", Location::new(1, 1))),
    };
    let permissions = document.get("permissions").map(|node| parse_permissions(node, "permissions")).transpose()?;
    let concurrency = document.get("concurrency").map(parse_concurrency).transpose()?;
    let Some((jobs_key, jobs_node)) = document.entry("jobs") else {
        return Err(WorkflowError::parse("the workflow has no `jobs` section", Location::new(1, 1)));
    };
    let jobs = parse_jobs(jobs_key, jobs_node, limits, deadline)?;
    Ok(Workflow { name, run_name, on, permissions, concurrency, jobs, document })
}

fn error(message: impl Into<String>, at: Location) -> WorkflowError {
    WorkflowError::parse(message, at)
}

fn optional_text(mapping: &Mapping, key: &str) -> Result<Option<String>, WorkflowError> {
    match mapping.get(key) {
        None => Ok(None),
        Some(node) if node.is_null() => Ok(None),
        Some(node) => node.as_text().map(Some).ok_or_else(|| error(format!("`{key}` must be a string"), node.at)),
    }
}

fn mapping<'a>(node: &'a Node, path: &str) -> Result<&'a Mapping, WorkflowError> {
    node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a mapping"), node.at))
}

/// A string, or a list of strings (GitHub accepts either for most lists).
fn strings(node: &Node, path: &str) -> Result<Vec<String>, WorkflowError> {
    let expected = || error(format!("`{path}` must be a string or a list of strings"), node.at);
    match &node.value {
        Value::Scalar(scalar) if scalar.value != ScalarValue::Null => Ok(vec![scalar.text()]),
        Value::Sequence(items) => items.iter().map(|item| item.as_text().ok_or_else(expected)).collect(),
        _ => Err(expected()),
    }
}

fn boolean(node: &Node, path: &str) -> Result<bool, WorkflowError> {
    match node.as_scalar().map(|scalar| &scalar.value) {
        Some(ScalarValue::Bool(value)) => Ok(*value),
        _ => Err(error(format!("`{path}` must be true or false"), node.at)),
    }
}

fn parse_on(node: &Node) -> Result<On, WorkflowError> {
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

fn add_event(on: &mut On, event: &str, config: Option<&Node>, at: Location) -> Result<(), WorkflowError> {
    let config = config.filter(|node| !node.is_null());
    match event {
        "push" => on.push = Some(config.map(parse_push).transpose()?.unwrap_or_default()),
        "pull_request" => on.pull_request = Some(config.map(parse_pull_request).transpose()?.unwrap_or_default()),
        "workflow_dispatch" => on.workflow_dispatch = Some(config.map(parse_dispatch).transpose()?.unwrap_or_default()),
        "schedule" => match config {
            Some(node) => on.schedule = parse_schedule(node)?,
            None => return Err(error("`on.schedule` needs a list of `cron` entries", at)),
        },
        "" => return Err(error("an event name must not be empty", at)),
        other => on.other.push(other.to_owned()),
    }
    Ok(())
}

/// Reads the filter pair `name` / `name-ignore` of an event.
fn filter(filters: &Mapping, event: &str, name: &str) -> Result<Filter, WorkflowError> {
    let ignore = format!("{name}-ignore");
    match (filters.entry(name), filters.entry(&ignore)) {
        (Some((key, _)), Some(_)) => Err(error(format!("`on.{event}` cannot have both `{name}` and `{ignore}`"), key.at)),
        (Some((_, node)), None) => Ok(Filter::Include(patterns(node, &format!("on.{event}.{name}"))?)),
        (None, Some((_, node))) => Ok(Filter::Ignore(patterns(node, &format!("on.{event}.{ignore}"))?)),
        (None, None) => Ok(Filter::Absent),
    }
}

fn patterns(node: &Node, path: &str) -> Result<Patterns, WorkflowError> {
    let values = strings(node, path)?;
    Patterns::compile(&values).map_err(|problem| error(format!("`{path}`: {problem}"), node.at))
}

fn only_keys(filters: &Mapping, event: &str, allowed: &[&str]) -> Result<(), WorkflowError> {
    match filters.iter().find(|(key, _)| !allowed.contains(&key.name.as_str())) {
        Some((key, _)) => {
            Err(error(format!("`on.{event}` does not support `{}` (rbit supports {})", key.name, allowed.join(", ")), key.at))
        }
        None => Ok(()),
    }
}

fn parse_push(node: &Node) -> Result<PushFilters, WorkflowError> {
    let filters = mapping(node, "on.push")?;
    only_keys(filters, "push", &["branches", "branches-ignore", "tags", "tags-ignore", "paths", "paths-ignore"])?;
    Ok(PushFilters {
        branches: filter(filters, "push", "branches")?,
        tags: filter(filters, "push", "tags")?,
        paths: filter(filters, "push", "paths")?,
    })
}

fn parse_pull_request(node: &Node) -> Result<PullRequestFilters, WorkflowError> {
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

fn parse_dispatch(node: &Node) -> Result<Dispatch, WorkflowError> {
    let config = mapping(node, "on.workflow_dispatch")?;
    only_keys(config, "workflow_dispatch", &["inputs"])?;
    let Some(inputs) = config.get("inputs").filter(|node| !node.is_null()) else {
        return Ok(Dispatch::default());
    };
    let inputs = mapping(inputs, "on.workflow_dispatch.inputs")?;
    if inputs.len() > MAX_DISPATCH_INPUTS {
        return Err(WorkflowError::limit(format!("`workflow_dispatch` may have at most {MAX_DISPATCH_INPUTS} inputs"), Some(node.at)));
    }
    let inputs = inputs.iter().map(|(key, node)| parse_input(key, node)).collect::<Result<_, _>>()?;
    Ok(Dispatch { inputs })
}

fn parse_input(key: &Key, node: &Node) -> Result<DispatchInput, WorkflowError> {
    let path = format!("on.workflow_dispatch.inputs.{}", key.name);
    if key.name.is_empty() || !key.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(error(format!("`{path}`: input names may only contain letters, digits, '_' and '-'"), key.at));
    }
    let empty = Mapping::default();
    let config = if node.is_null() { &empty } else { mapping(node, &path)? };
    let required = config.get("required").map(|node| boolean(node, &format!("{path}.required"))).transpose()?.unwrap_or(false);
    let kind = match optional_text(config, "type")?.as_deref() {
        None | Some("string") => InputKind::String,
        Some("boolean") => InputKind::Boolean,
        Some("number") => InputKind::Number,
        Some("environment") => InputKind::Environment,
        Some("choice") => {
            let options = config
                .get("options")
                .map(|node| strings(node, &format!("{path}.options")))
                .transpose()?
                .filter(|options| !options.is_empty())
                .ok_or_else(|| error(format!("`{path}`: a choice input needs `options`"), node.at))?;
            InputKind::Choice(options)
        }
        Some(other) => return Err(error(format!("`{path}.type`: `{other}` is not an input type"), node.at)),
    };
    let default = match config.get("default") {
        Some(node) if !node.is_null() => {
            Some(node.as_text().ok_or_else(|| error(format!("`{path}.default` must be a single value"), node.at))?)
        }
        _ => None,
    };
    if let (InputKind::Choice(options), Some(default)) = (&kind, &default)
        && !options.contains(default)
    {
        return Err(error(format!("`{path}.default` must be one of its options"), node.at));
    }
    Ok(DispatchInput { name: key.name.clone(), description: optional_text(config, "description")?, required, default, kind })
}

fn parse_schedule(node: &Node) -> Result<Vec<Schedule>, WorkflowError> {
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

fn parse_permissions(node: &Node, path: &str) -> Result<Permissions, WorkflowError> {
    match &node.value {
        Value::Scalar(scalar) => match scalar.text().as_str() {
            "read-all" => Ok(Permissions::uniform(Permission::Read)),
            "write-all" => Ok(Permissions::uniform(Permission::Write)),
            _ => Err(error(format!("`{path}` must be read-all, write-all or a mapping"), node.at)),
        },
        Value::Mapping(scopes) => {
            let mut permissions = Permissions::uniform(Permission::None);
            for (key, value) in scopes.iter() {
                let level = match value.as_text().as_deref() {
                    Some("read") => Permission::Read,
                    Some("write") => Permission::Write,
                    Some("none") => Permission::None,
                    _ => return Err(error(format!("`{path}.{}` must be read, write or none", key.name), value.at)),
                };
                match key.name.as_str() {
                    "contents" => permissions.contents = level,
                    "issues" => permissions.issues = level,
                    "pull-requests" => permissions.pull_requests = level,
                    _ => {}
                }
            }
            Ok(permissions)
        }
        Value::Sequence(_) => Err(error(format!("`{path}` must be read-all, write-all or a mapping"), node.at)),
    }
}

fn parse_concurrency(node: &Node) -> Result<Concurrency, WorkflowError> {
    match &node.value {
        Value::Scalar(_) => {
            let group = node.as_text().ok_or_else(|| error("`concurrency` must not be empty", node.at))?;
            Ok(Concurrency { group, cancel_in_progress: false, at: node.at })
        }
        Value::Mapping(config) => {
            only_keys_at(config, "concurrency", &["group", "cancel-in-progress"])?;
            let group = config
                .get("group")
                .and_then(Node::as_text)
                .filter(|group| !group.is_empty())
                .ok_or_else(|| error("`concurrency` needs a `group`", node.at))?;
            let cancel_in_progress = match config.get("cancel-in-progress") {
                None => false,
                Some(value) if value.as_scalar().is_some_and(|scalar| matches!(scalar.value, ScalarValue::String(_))) => {
                    return Err(error(
                        "`concurrency.cancel-in-progress` must be true or false; rbit does not evaluate expressions there",
                        value.at,
                    ));
                }
                Some(value) => boolean(value, "concurrency.cancel-in-progress")?,
            };
            Ok(Concurrency { group, cancel_in_progress, at: node.at })
        }
        Value::Sequence(_) => Err(error("`concurrency` must be a group name or a mapping", node.at)),
    }
}

fn only_keys_at(config: &Mapping, path: &str, allowed: &[&str]) -> Result<(), WorkflowError> {
    match config.iter().find(|(key, _)| !allowed.contains(&key.name.as_str())) {
        Some((key, _)) => Err(error(format!("`{path}` does not support `{}`", key.name), key.at)),
        None => Ok(()),
    }
}

fn valid_job_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    id.len() <= MAX_JOB_ID_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn parse_jobs(key: &Key, node: &Node, limits: &Limits, deadline: &Deadline) -> Result<Vec<Job>, WorkflowError> {
    let entries =
        node.as_mapping().filter(|jobs| !jobs.is_empty()).ok_or_else(|| error("`jobs` must be a mapping of at least one job", key.at))?;
    if entries.len() > limits.max_jobs {
        return Err(WorkflowError::limit(format!("a workflow may have at most {} jobs", limits.max_jobs), Some(key.at)));
    }
    let mut jobs = Vec::with_capacity(entries.len());
    for (key, node) in entries.iter() {
        deadline.check()?;
        jobs.push(parse_job(key, node)?);
    }
    check_needs(&jobs)?;
    Ok(jobs)
}

fn parse_job(key: &Key, node: &Node) -> Result<Job, WorkflowError> {
    let id = key.name.clone();
    if !valid_job_id(&id) {
        return Err(error(
            format!(
                "job id `{id}` must start with a letter or '_' and contain only letters, digits, '_' and '-' (at most {MAX_JOB_ID_BYTES})"
            ),
            key.at,
        ));
    }
    let path = format!("jobs.{id}");
    let config = node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a mapping"), node.at))?;
    if let Some((key, _)) = config.entry("uses") {
        return Err(error(format!("`{path}.uses`: reusable workflows are not supported"), key.at));
    }
    let name = optional_text(config, "name").map_err(|_| error(format!("`{path}.name` must be a string"), node.at))?;
    let needs = config.get("needs").map(|node| strings(node, &format!("{path}.needs"))).transpose()?.unwrap_or_default();
    let runs_on = match config.get("runs-on") {
        None => return Err(error(format!("`{path}` needs `runs-on`"), key.at)),
        Some(node) => parse_runs_on(node, &format!("{path}.runs-on"))?,
    };
    let matrix = match config.get("strategy").filter(|node| !node.is_null()) {
        None => None,
        Some(strategy) => mapping(strategy, &format!("{path}.strategy"))?
            .get("matrix")
            .filter(|node| !node.is_null())
            .map(|node| parse_matrix(node, &format!("{path}.strategy.matrix")))
            .transpose()?,
    };
    let permissions = config.get("permissions").map(|node| parse_permissions(node, &format!("{path}.permissions"))).transpose()?;
    Ok(Job { id, name, needs, runs_on, matrix, permissions, at: key.at, node: config.clone() })
}

fn parse_runs_on(node: &Node, path: &str) -> Result<Vec<String>, WorkflowError> {
    let labels = match &node.value {
        Value::Mapping(config) => {
            if config.get("group").is_some() && config.get("labels").is_none() {
                return Err(error(format!("`{path}`: runner groups are not supported; use `labels`"), node.at));
            }
            config.get("labels").map(|labels| strings(labels, &format!("{path}.labels"))).transpose()?.unwrap_or_default()
        }
        _ => strings(node, path)?,
    };
    if labels.is_empty() || labels.iter().any(String::is_empty) {
        return Err(error(format!("`{path}` must name at least one label, and no empty one"), node.at));
    }
    Ok(labels)
}

fn parse_matrix(node: &Node, path: &str) -> Result<Matrix, WorkflowError> {
    let Value::Mapping(config) = &node.value else {
        return Err(error(format!("`{path}` must be a mapping; rbit does not evaluate expressions there"), node.at));
    };
    let mut dimensions = Vec::new();
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for (key, value) in config.iter() {
        let combinations = |list: &mut Vec<Mapping>| -> Result<(), WorkflowError> {
            let items = value.as_sequence().ok_or_else(|| error(format!("`{path}.{}` must be a list of mappings", key.name), value.at))?;
            for item in items {
                list.push(mapping(item, &format!("{path}.{}[]", key.name))?.clone());
            }
            Ok(())
        };
        match key.name.as_str() {
            "include" => combinations(&mut include)?,
            "exclude" => combinations(&mut exclude)?,
            name => {
                let values = value
                    .as_sequence()
                    .ok_or_else(|| error(format!("`{path}.{name}` must be a list; rbit does not evaluate expressions there"), value.at))?;
                dimensions.push((name.to_owned(), values.to_vec()));
            }
        }
    }
    // GitHub refuses an exclude naming a key that is not a dimension (it silently skips
    // such an include key instead).
    for entry in &exclude {
        if let Some((key, _)) = entry.iter().find(|(key, _)| !dimensions.iter().any(|(name, _)| *name == key.name)) {
            return Err(error(format!("`{path}.exclude`: `{}` is not a matrix dimension", key.name), key.at));
        }
    }
    if dimensions.is_empty() && include.is_empty() {
        return Err(error(format!("`{path}` must have a dimension or an `include`"), node.at));
    }
    Ok(Matrix { dimensions, include, exclude, at: node.at })
}

/// Every `needs` names another job, and the jobs form no cycle.
fn check_needs(jobs: &[Job]) -> Result<(), WorkflowError> {
    let ids: BTreeSet<&str> = jobs.iter().map(|job| job.id.as_str()).collect();
    for job in jobs {
        if let Some(missing) = job.needs.iter().find(|need| !ids.contains(need.as_str())) {
            return Err(error(format!("job `{}` needs `{missing}`, which does not exist", job.id), job.at));
        }
        if job.needs.contains(&job.id) {
            return Err(error(format!("job `{}` needs itself", job.id), job.at));
        }
    }
    // Kahn's algorithm: whatever cannot be ordered is on a cycle.
    let mut waiting: BTreeMap<&str, usize> = jobs.iter().map(|job| (job.id.as_str(), job.needs.len())).collect();
    let mut ready: Vec<&str> = waiting.iter().filter(|(_, count)| **count == 0).map(|(id, _)| *id).collect();
    while let Some(done) = ready.pop() {
        waiting.remove(done);
        for job in jobs.iter().filter(|job| job.needs.iter().any(|need| need == done)) {
            if let Some(count) = waiting.get_mut(job.id.as_str()) {
                *count -= job.needs.iter().filter(|need| *need == done).count();
                if *count == 0 {
                    ready.push(&job.id);
                }
            }
        }
    }
    match jobs.iter().find(|job| waiting.contains_key(job.id.as_str())) {
        Some(job) => Err(error(format!("job `{}` is part of a `needs` cycle", job.id), job.at)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Workflow {
        parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap_or_else(|error| panic!("{error}\n{source}"))
    }

    fn parse_err(source: &str) -> WorkflowError {
        parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).expect_err(source)
    }

    const CI: &str = "\
name: CI
run-name: Build ${{ github.ref }}
on:
  push:
    branches: [main, 'release/**']
    paths-ignore: ['docs/**']
  pull_request:
    types: [opened, synchronize, ready_for_review]
    branches: main
  workflow_dispatch:
    inputs:
      level:
        type: choice
        options: [low, high]
        default: low
        required: true
      dry-run:
        type: boolean
  schedule:
    - cron: '0 3 * * 1'
    - cron: '*/30 * * * *'
permissions:
  contents: write
  pull-requests: read
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs:
  build:
    name: Build ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, debian-12]
        rust: [stable, 1.94]
        include:
          - os: debian-12
            experimental: true
        exclude:
          - os: debian-12
            rust: 1.94
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
  deploy:
    needs: build
    runs-on: [self-hosted, linux]
    permissions: read-all
    steps:
      - run: ./deploy
";

    #[test]
    fn a_complete_workflow() {
        let workflow = parse_ok(CI);
        assert_eq!(workflow.name.as_deref(), Some("CI"));
        assert_eq!(workflow.run_name.as_deref(), Some("Build ${{ github.ref }}"));
        let push = workflow.on.push.as_ref().unwrap();
        assert!(matches!(&push.branches, Filter::Include(patterns) if patterns.iter().count() == 2));
        assert!(push.tags.is_absent());
        assert!(matches!(&push.paths, Filter::Ignore(_)));
        let pull = workflow.on.pull_request.as_ref().unwrap();
        assert_eq!(pull.types.as_deref(), Some(&[ActivityType::Opened, ActivityType::Synchronize, ActivityType::ReadyForReview][..]));
        let dispatch = workflow.on.workflow_dispatch.as_ref().unwrap();
        assert_eq!(
            dispatch.inputs,
            [
                DispatchInput {
                    name: "level".into(),
                    description: None,
                    required: true,
                    default: Some("low".into()),
                    kind: InputKind::Choice(vec!["low".into(), "high".into()]),
                },
                DispatchInput { name: "dry-run".into(), description: None, required: false, default: None, kind: InputKind::Boolean },
            ]
        );
        let crons: Vec<_> = workflow.on.schedule.iter().map(|entry| entry.cron.as_str()).collect();
        assert_eq!(crons, ["0 3 * * 1", "*/30 * * * *"]);
        assert_eq!(workflow.on.schedule[0].at.line, 20);
        assert_eq!(
            workflow.permissions,
            Some(Permissions { contents: Permission::Write, issues: Permission::None, pull_requests: Permission::Read })
        );
        assert_eq!(
            workflow.concurrency,
            Some(Concurrency { group: "ci-${{ github.ref }}".into(), cancel_in_progress: true, at: Location::new(26, 3) })
        );
        let ids: Vec<_> = workflow.jobs.iter().map(|job| job.id.as_str()).collect();
        assert_eq!(ids, ["build", "deploy"]);
        let build = workflow.job("build").unwrap();
        assert_eq!(build.name.as_deref(), Some("Build ${{ matrix.os }}"));
        assert_eq!(build.runs_on, ["${{ matrix.os }}"]);
        let matrix = build.matrix.as_ref().unwrap();
        let dimensions: Vec<_> = matrix.dimensions.iter().map(|(name, values)| (name.as_str(), values.len())).collect();
        assert_eq!(dimensions, [("os", 2), ("rust", 2)]);
        assert_eq!((matrix.include.len(), matrix.exclude.len()), (1, 1));
        let deploy = workflow.job("deploy").unwrap();
        assert_eq!(deploy.needs, ["build"]);
        assert_eq!(deploy.runs_on, ["self-hosted", "linux"]);
        assert_eq!(deploy.permissions, Some(Permissions::uniform(Permission::Read)));
    }

    #[test]
    fn short_forms_of_on() {
        let workflow = parse_ok("on: push\njobs: {a: {runs-on: x}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_none());
        let workflow = parse_ok("on: [push, pull_request, issues, workflow_dispatch]\njobs: {a: {runs-on: x}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_some() && workflow.on.workflow_dispatch.is_some());
        assert_eq!(workflow.on.other, ["issues"]);
        let workflow = parse_ok("'on':\n  push:\n  pull_request: {}\njobs: {a: {runs-on: x}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.as_ref().unwrap().types.is_none());
    }

    #[test]
    fn permissions_forms() {
        let job = |permissions: &str| {
            let source = format!("on: push\njobs:\n  a:\n    runs-on: x\n    permissions: {permissions}\n");
            parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).map(|workflow| workflow.jobs[0].permissions)
        };
        assert_eq!(job("read-all").unwrap(), Some(Permissions::uniform(Permission::Read)));
        assert_eq!(job("write-all").unwrap(), Some(Permissions::uniform(Permission::Write)));
        assert_eq!(job("{}").unwrap(), Some(Permissions::uniform(Permission::None)));
        assert_eq!(
            job("{issues: write, id-token: write, packages: read}").unwrap(),
            Some(Permissions { issues: Permission::Write, ..Permissions::uniform(Permission::None) })
        );
        assert_eq!(job("{contents: none}").unwrap(), Some(Permissions::uniform(Permission::None)));
        assert!(job("admin").is_err());
        assert!(job("{contents: admin}").is_err());
        assert!(job("[read]").is_err());
    }

    #[test]
    fn errors_point_at_the_problem() {
        let cases: &[(&str, &str, (u32, u32))] = &[
            ("jobs: {a: {runs-on: x}}\n", "no `on` section", (1, 1)),
            ("on: push\n", "no `jobs` section", (1, 1)),
            ("on: push\njobs: {}\n", "at least one job", (2, 1)),
            ("- a\n", "must be a mapping", (1, 1)),
            ("on: push\njobs:\n  1st: {runs-on: x}\n", "job id `1st`", (3, 3)),
            ("on: push\njobs:\n  a b: {runs-on: x}\n", "job id `a b`", (3, 3)),
            ("on: push\njobs:\n  a: {steps: []}\n", "needs `runs-on`", (3, 3)),
            ("on: push\njobs:\n  a: {runs-on: []}\n", "at least one label", (3, 16)),
            ("on: push\njobs:\n  a: {runs-on: {group: big}}\n", "runner groups", (3, 16)),
            ("on: push\njobs:\n  a: {uses: ./.github/workflows/x.yml}\n", "reusable workflows", (3, 7)),
            ("on: push\njobs:\n  a: {runs-on: x, needs: b}\n", "needs `b`, which does not exist", (3, 3)),
            ("on: push\njobs:\n  a: {runs-on: x, needs: a}\n", "needs itself", (3, 3)),
            ("on: push\njobs:\n  a: {runs-on: x, needs: b}\n  b: {runs-on: x, needs: c}\n  c: {runs-on: x, needs: a}\n", "cycle", (3, 3)),
            ("on:\n  push:\n    branches: [a]\n    branches-ignore: [b]\njobs: {a: {runs-on: x}}\n", "both `branches`", (3, 5)),
            ("on:\n  push:\n    branches: ['[]']\njobs: {a: {runs-on: x}}\n", "invalid pattern", (3, 15)),
            ("on:\n  push:\n    types: [created]\njobs: {a: {runs-on: x}}\n", "does not support `types`", (3, 5)),
            ("on:\n  pull_request:\n    types: [opend]\njobs: {a: {runs-on: x}}\n", "`opend` is not", (3, 12)),
            ("on:\n  schedule: '* * * * *'\njobs: {a: {runs-on: x}}\n", "list of `cron`", (2, 13)),
            ("on:\n  schedule:\n    - crn: x\njobs: {a: {runs-on: x}}\n", "`cron` string", (3, 7)),
            ("on:\n  schedule:\n    - cron: '61 * * * *'\njobs: {a: {runs-on: x}}\n", "between 0 and 59", (3, 13)),
            ("on:\n  schedule:\n    - cron: '* * * * *'\njobs: {a: {runs-on: x}}\n", "more often than every 5 minutes", (3, 13)),
            ("on: push\nconcurrency: {cancel-in-progress: true}\njobs: {a: {runs-on: x}}\n", "needs a `group`", (2, 14)),
            ("on: push\nconcurrency: {group: g, cancel-in-progress: '${{ x }}'}\njobs: {a: {runs-on: x}}\n", "true or false", (2, 45)),
            ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: '${{ fromJSON(x) }}'}\n", "must be a mapping", (5, 24)),
            ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: linux}}\n", "must be a list", (5, 29)),
            (
                "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: [a], exclude: [{arch: x}]}}\n",
                "`arch` is not a matrix dimension",
                (5, 45),
            ),
            ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {}}\n", "a dimension or an `include`", (5, 24)),
            ("on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice}\njobs: {a: {runs-on: x}}\n", "needs `options`", (4, 10)),
            (
                "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice, options: [a], default: b}\njobs: {a: {runs-on: x}}\n",
                "one of its options",
                (4, 10),
            ),
            ("on:\n  workflow_dispatch:\n    inputs:\n      x: {type: file}\njobs: {a: {runs-on: x}}\n", "not an input type", (4, 10)),
            ("on:\n  workflow_dispatch:\n    inputs:\n      'a b': {}\njobs: {a: {runs-on: x}}\n", "input names", (4, 7)),
        ];
        for (source, message, (line, column)) in cases {
            let error = parse_err(source);
            assert!(error.message.contains(message), "{source}: {error}");
            assert_eq!(error.at, Some(Location { line: *line, column: *column }), "{source}: {error}");
            assert!(matches!(error.kind, ErrorKind::Parse), "{source}");
        }
    }

    #[test]
    fn limits_are_enforced() {
        let limits = Limits { max_bytes: 64, ..Limits::default() };
        let error = parse(&[b' '; 65], &limits, &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Limit);
        assert_eq!(parse(b"\xff", &Limits::default(), &Deadline::NONE).unwrap_err().kind, ErrorKind::Parse);
        let jobs = (0..=Limits::default().max_jobs).map(|index| format!("  j{index}: {{runs-on: x}}\n")).collect::<Vec<_>>().concat();
        assert_eq!(parse_err(&format!("on: push\njobs:\n{jobs}")).kind, ErrorKind::Limit);
        let inputs = (0..=MAX_DISPATCH_INPUTS).map(|index| format!("      i{index}: {{}}\n")).collect::<Vec<_>>().concat();
        let source = format!("on:\n  workflow_dispatch:\n    inputs:\n{inputs}jobs: {{a: {{runs-on: x}}}}\n");
        assert_eq!(parse_err(&source).kind, ErrorKind::Limit);
        let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
        assert_eq!(parse(CI.as_bytes(), &Limits::default(), &expired).unwrap_err().kind, ErrorKind::Timeout);
    }

    #[test]
    fn job_ids() {
        for valid in ["a", "_", "build-and-test", "A_1", &"a".repeat(MAX_JOB_ID_BYTES)] {
            assert!(valid_job_id(valid), "{valid}");
        }
        for invalid in ["", "1a", "-a", "a.b", "a b", "é", &"a".repeat(MAX_JOB_ID_BYTES + 1)] {
            assert!(!valid_job_id(invalid), "{invalid}");
        }
    }

    mod never_panics {
        use proptest::prelude::*;

        use super::*;

        fn yamlish() -> impl Strategy<Value = String> {
            let token = prop_oneof![
                Just("on".to_owned()),
                Just("jobs".to_owned()),
                Just("runs-on".to_owned()),
                Just("needs".to_owned()),
                Just("strategy".to_owned()),
                Just("matrix".to_owned()),
                Just("include".to_owned()),
                Just("push".to_owned()),
                Just("branches".to_owned()),
                Just("&a".to_owned()),
                Just("*a".to_owned()),
                Just("<<".to_owned()),
                Just("${{ matrix.x }}".to_owned()),
                "[a-z0-9]{1,6}",
            ];
            let piece = prop_oneof![
                token,
                Just(": ".to_owned()),
                Just("\n".to_owned()),
                Just("\n  ".to_owned()),
                Just("\n    ".to_owned()),
                Just("- ".to_owned()),
                Just("[".to_owned()),
                Just("]".to_owned()),
                Just("{".to_owned()),
                Just("}".to_owned()),
                Just(", ".to_owned()),
                Just("'".to_owned()),
                Just("\"".to_owned()),
                Just("|\n".to_owned()),
                Just("#".to_owned()),
            ];
            prop::collection::vec(piece, 0..64).prop_map(|pieces| pieces.concat())
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn on_arbitrary_bytes(source in prop::collection::vec(any::<u8>(), 0..512)) {
                let _ = parse(&source, &Limits::default(), &Deadline::NONE);
            }

            #[test]
            fn on_yaml_like_text(source in yamlish()) {
                let _ = parse(source.as_bytes(), &Limits::default(), &Deadline::NONE);
            }
        }
    }
}
