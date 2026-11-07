//! Job identity, limits, deadline checks and ordinary job composition.
use super::calls::{CALL_KEYS, CALL_UNSUPPORTED, parse_call};
use super::dependencies::check_needs;
use super::environment::{parse_defaults, parse_env};
use super::expressions::{Scope, amount, condition, flag, template_field};
use super::permissions::{parse_concurrency, parse_permissions};
use super::runtime::{parse_container, parse_environment, parse_outputs, parse_runs_on, parse_services};
use super::steps::parse_steps;
use super::strategy::parse_strategy;
use super::syntax::{check_keys, error, strings};
use crate::error::{Location, WorkflowError};
use crate::limits::{Deadline, Limits};
use crate::model::{Env, Job, RunDefaults};
use crate::yaml::{Key, Mapping, Node};
use std::collections::BTreeMap;

/// Longest job id, in bytes.
pub const MAX_JOB_ID_BYTES: usize = 100;

/// The keys of a job.
pub(super) const JOB_KEYS: [&str; 16] = [
    "name",
    "needs",
    "if",
    "runs-on",
    "permissions",
    "env",
    "defaults",
    "outputs",
    "timeout-minutes",
    "continue-on-error",
    "concurrency",
    "strategy",
    "container",
    "services",
    "steps",
    "environment",
];

/// GitHub's job keys that rbit lacks, and why.
pub(super) const JOB_UNSUPPORTED: [(&str, &str); 4] = [
    ("with", ": only a job that calls a workflow (`uses`) takes it"),
    ("secrets", ": only a job that calls a workflow (`uses`) takes it"),
    ("cancel-timeout-minutes", ""),
    ("snapshot", ""),
];

pub(crate) fn valid_job_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    id.len() <= MAX_JOB_ID_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub(super) fn parse_jobs(key: &Key, node: &Node, limits: &Limits, deadline: &Deadline) -> Result<Vec<Job>, WorkflowError> {
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

/// A job's `needs`, each named once.
pub(super) fn parse_needs(config: &Mapping, path: &str, at: Location) -> Result<Vec<String>, WorkflowError> {
    let needs = config.get("needs").map(|node| strings(node, &format!("{path}.needs"))).transpose()?.unwrap_or_default();
    if let Some(repeated) = needs.iter().enumerate().find_map(|(index, need)| needs[..index].contains(need).then_some(need)) {
        return Err(error(format!("`{path}.needs` names `{repeated}` twice"), config.get("needs").map_or(at, |node| node.at)));
    }
    Ok(needs)
}

/// A job's `if`, checked.
pub(super) fn job_condition(config: &Mapping, path: &str) -> Result<Option<String>, WorkflowError> {
    let Some(node) = config.get("if").filter(|node| !node.is_null()) else { return Ok(None) };
    let text = node.as_text().ok_or_else(|| error(format!("`{path}.if` must be a condition"), node.at))?;
    condition(&text, Scope::JobCondition, &format!("{path}.if"), node.at)?;
    Ok(Some(text))
}

pub(super) fn parse_job(key: &Key, node: &Node) -> Result<Job, WorkflowError> {
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
    // Actions: reusable workflows.
    let calls = config.get("uses").is_some();
    if calls {
        check_keys(config, &path, &CALL_KEYS, &CALL_UNSUPPORTED)?;
    } else {
        check_keys(config, &path, &JOB_KEYS, &JOB_UNSUPPORTED)?;
    }
    let field = |key: &str| format!("{path}.{key}");
    let name = template_field(config, "name", &field("name"), Scope::Job)?;
    let needs = parse_needs(config, &path, key.at)?;
    let condition = job_condition(config, &path)?;
    let permissions = config.get("permissions").map(|node| parse_permissions(node, &field("permissions"))).transpose()?;
    // Actions: reusable workflows.
    if calls {
        let strategy = config.get("strategy").map(|node| parse_strategy(node, &field("strategy"))).transpose()?.unwrap_or_default();
        return Ok(Job {
            id,
            name,
            needs,
            condition,
            runs_on: Vec::new(),
            permissions,
            env: Env::default(),
            defaults: RunDefaults::default(),
            outputs: BTreeMap::new(),
            timeout_minutes: None,
            continue_on_error: None,
            concurrency: None,
            strategy,
            container: None,
            services: Vec::new(),
            steps: Vec::new(),
            environment: None,
            call: Some(parse_call(config, &path)?),
            at: key.at,
        });
    }
    let runs_on = match config.get("runs-on") {
        None => return Err(error(format!("`{path}` needs `runs-on`"), key.at)),
        Some(node) => parse_runs_on(node, &field("runs-on"))?,
    };
    let env = config.get("env").map(|node| parse_env(node, &field("env"))).transpose()?.unwrap_or_default();
    let defaults = config.get("defaults").map(|node| parse_defaults(node, &field("defaults"))).transpose()?.unwrap_or_default();
    let outputs = config.get("outputs").map(|node| parse_outputs(node, &field("outputs"))).transpose()?.unwrap_or_default();
    let timeout_minutes = config
        .get("timeout-minutes")
        .filter(|node| !node.is_null())
        .map(|node| amount(node, &field("timeout-minutes"), Scope::Job, false))
        .transpose()?;
    let continue_on_error = config
        .get("continue-on-error")
        .filter(|node| !node.is_null())
        .map(|node| flag(node, &field("continue-on-error"), Scope::Job))
        .transpose()?;
    let concurrency = config.get("concurrency").map(|node| parse_concurrency(node, &field("concurrency"), Scope::Job)).transpose()?;
    let strategy = config.get("strategy").map(|node| parse_strategy(node, &field("strategy"))).transpose()?.unwrap_or_default();
    let container =
        config.get("container").filter(|node| !node.is_null()).map(|node| parse_container(node, &field("container"))).transpose()?;
    let services = config.get("services").map(|node| parse_services(node, &field("services"))).transpose()?.unwrap_or_default();
    let steps = match config.get("steps") {
        None => return Err(error(format!("`{path}` needs `steps`"), key.at)),
        Some(node) => parse_steps(node, &field("steps"))?,
    };
    let environment =
        config.get("environment").filter(|node| !node.is_null()).map(|node| parse_environment(node, &field("environment"))).transpose()?;
    Ok(Job {
        id,
        name,
        needs,
        condition,
        runs_on,
        permissions,
        env,
        defaults,
        outputs,
        timeout_minutes,
        continue_on_error,
        concurrency,
        strategy,
        container,
        services,
        steps,
        environment,
        call: None,
        at: key.at,
    })
}
