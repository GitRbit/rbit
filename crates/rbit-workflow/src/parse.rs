//! Workflow file → [`Workflow`], with every problem reported at its place in the file.
//!
//! Every key of §12.2 is read into the typed model, and every other key is an error at
//! every level, because a typo must not silently change what a job does. GitHub keys rbit
//! deliberately lacks (`workflow_call`, `jobs.<id>.uses`, `environment`, `secrets: inherit`,
//! `container.options`, `services.*.options` and `services.*.volumes`, …) say "not supported
//! by rbit".
//!
//! Structural checks: the shape of `on`, filters that compile, job ids, `runs-on`, `needs`
//! that name existing jobs without a cycle, a matrix that is a mapping of lists,
//! `permissions`, `concurrency`, containers, services, and steps with exactly one of `run`
//! and `uses`. Every `${{ }}` is parsed here, and may only use the contexts its field is
//! evaluated with (§12.4): `run-name` and `strategy` see `github`, `inputs` and `vars`; the
//! fields the server evaluates per job also `matrix` and `strategy`; a job's `if` also
//! `needs`; what the runner evaluates sees everything. Status functions only work in `if`
//! and `hashFiles` only on the runner. Expressions are left as written; planning evaluates
//! the server's and the runner evaluates the rest.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    builtins,
    cron::{Cron, MIN_INTERVAL_MINUTES},
    error::{ErrorKind, Location, WorkflowError},
    expr::{Condition, Expr, Function, Template},
    limits::{Deadline, Limits},
    model::{
        ActionStep, ActivityType, Builtin, CallInput, CallInputKind, CallSecrets, CallTarget, Concurrency, Container, Dispatch,
        DispatchInput, Env, EnvironmentRef, Filter, InputKind, Job, JobCall, Matrix, On, Permission, Permissions, PullRequestFilters,
        PushFilters, RegistryCredentials, RepositoryDispatchFilters, RunDefaults, Schedule, Service, Shell, Step, StepAction, Strategy,
        Workflow, WorkflowCall, WorkflowRunFilters, WorkflowRunType,
    },
    pattern::Patterns,
    spec::{MAX_NAME_BYTES, MAX_SERVICES, MAX_STEPS},
    yaml::{self, Key, Mapping, Node, ScalarValue, Value},
};

/// Most `workflow_dispatch` inputs.
pub const MAX_DISPATCH_INPUTS: usize = 25;
/// Longest job id, in bytes.
pub const MAX_JOB_ID_BYTES: usize = 100;

/// The keys of a workflow.
const WORKFLOW_KEYS: [&str; 8] = ["name", "run-name", "on", "env", "defaults", "permissions", "concurrency", "jobs"];
/// The keys of a job.
const JOB_KEYS: [&str; 16] = [
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
const JOB_UNSUPPORTED: [(&str, &str); 4] = [
    ("with", ": only a job that calls a workflow (`uses`) takes it"),
    ("secrets", ": only a job that calls a workflow (`uses`) takes it"),
    ("cancel-timeout-minutes", ""),
    ("snapshot", ""),
];
// Actions: reusable workflows.
/// The keys of a job that calls a workflow (`uses`), as on GitHub.
const CALL_KEYS: [&str; 8] = ["name", "uses", "with", "secrets", "strategy", "needs", "if", "permissions"];
/// Keys of other jobs that a calling job cannot have, and why.
const CALL_UNSUPPORTED: [(&str, &str); 12] = [
    ("runs-on", ": the called workflow's jobs say where they run"),
    ("steps", ": the called workflow's jobs have the steps"),
    ("env", ": the called workflow does not see the caller's `env`"),
    ("defaults", ": the called workflow has its own"),
    ("outputs", ": the called workflow declares them under `on.workflow_call.outputs`"),
    ("timeout-minutes", ": set it on the called workflow's jobs"),
    ("continue-on-error", ": set it on the called workflow's jobs"),
    ("container", ": set it on the called workflow's jobs"),
    ("services", ": set it on the called workflow's jobs"),
    ("environment", ": set it on the called workflow's jobs"),
    ("concurrency", " on a job that calls a workflow yet; set it on the called workflow's jobs"),
    ("cancel-timeout-minutes", ""),
];
/// The keys of a step.
const STEP_KEYS: [&str; 11] =
    ["id", "name", "if", "run", "shell", "working-directory", "env", "uses", "with", "continue-on-error", "timeout-minutes"];
/// The container keys GitHub has and rbit lacks.
const CONTAINER_UNSUPPORTED: [(&str, &str); 1] = [("ports", ": services reach the job container by name on the job's own network")];
/// The service keys GitHub has and rbit lacks.
const SERVICE_UNSUPPORTED: [(&str, &str); 1] = [("ports", ": the job reaches a service by its id as host name")];
/// The keys of a container and of a service.
const CONTAINER_KEYS: [&str; 5] = ["image", "env", "credentials", "options", "volumes"];
/// Every event GitHub documents for `on`. rbit starts runs for some of them
/// ([`crate::trigger`]); the others are accepted and never fire.
const GITHUB_EVENTS: [&str; 35] = [
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
    check_keys(&document, "", &WORKFLOW_KEYS, &[])?;
    let name = optional_text(&document, "name")?;
    let run_name = text_field(&document, "run-name", "run-name")?;
    if let (Some(run_name), Some(node)) = (&run_name, document.get("run-name")) {
        template(run_name, Scope::Run, "run-name", node.at)?;
    }
    let on = match document.entry("on") {
        Some((_, node)) => parse_on(node)?,
        None => return Err(WorkflowError::parse("the workflow has no `on` section", Location::new(1, 1))),
    };
    let env = document.get("env").map(|node| parse_env(node, "env")).transpose()?.unwrap_or_default();
    let defaults = document.get("defaults").map(|node| parse_defaults(node, "defaults")).transpose()?.unwrap_or_default();
    let permissions = document.get("permissions").map(|node| parse_permissions(node, "permissions")).transpose()?;
    let concurrency = document.get("concurrency").map(|node| parse_concurrency(node, "concurrency", Scope::Run)).transpose()?;
    let Some((jobs_key, jobs_node)) = document.entry("jobs") else {
        return Err(WorkflowError::parse("the workflow has no `jobs` section", Location::new(1, 1)));
    };
    let jobs = parse_jobs(jobs_key, jobs_node, limits, deadline)?;
    Ok(Workflow { name, run_name, on, env, defaults, permissions, concurrency, jobs })
}

fn error(message: impl Into<String>, at: Location) -> WorkflowError {
    WorkflowError::parse(message, at)
}

/// Refuses every key of `mapping` that is not `allowed`; a key of `unsupported` is a GitHub
/// key rbit lacks, with the reason.
fn check_keys(mapping: &Mapping, path: &str, allowed: &[&str], unsupported: &[(&str, &str)]) -> Result<(), WorkflowError> {
    let Some((key, _)) = mapping.iter().find(|(key, _)| !allowed.contains(&key.name.as_str())) else {
        return Ok(());
    };
    let full = if path.is_empty() { key.name.clone() } else { format!("{path}.{}", key.name) };
    if let Some((_, why)) = unsupported.iter().find(|(name, _)| *name == key.name) {
        return Err(error(format!("`{full}` is not supported by rbit{why}"), key.at));
    }
    let owner = if path.is_empty() { "a workflow".to_owned() } else { format!("`{path}`") };
    Err(error(format!("{owner} does not support `{}`; it supports {}", key.name, allowed.join(", ")), key.at))
}

/// Where a field is evaluated (§12.4), which decides the contexts and functions it may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The server, when the run is created: `run-name`, workflow `concurrency`.
    Run,
    // Actions: plans from needs.
    /// The server: a job's `strategy`, which may read `needs` (then it is planned when they
    /// are done).
    Strategy,
    /// The server, per planned job: job `name`, `runs-on`, `timeout-minutes`,
    /// `continue-on-error`, `concurrency`, `container.image`, `services.*.image`.
    Job,
    /// The server, when the job unblocks: job `if`.
    JobCondition,
    /// The runner.
    Runner,
    /// The runner: step `if`.
    StepCondition,
    // Actions: reusable workflows.
    /// The server, for a job that calls a workflow: `with`.
    Call,
    /// The server, for a job that calls a workflow: `secrets`.
    CallSecrets,
    /// The server: a called workflow's `outputs`.
    CallOutputs,
}

/// What the runner provides: every context but `jobs`, which only a called workflow's
/// `outputs` read.
const RUNNER_CONTEXTS: [&str; 11] = ["github", "env", "vars", "job", "steps", "runner", "secrets", "strategy", "matrix", "needs", "inputs"];

impl Scope {
    fn contexts(self) -> &'static [&'static str] {
        match self {
            Self::Run => &["github", "inputs", "vars"],
            Self::Strategy => &["github", "inputs", "vars", "needs"],
            // Actions: plans from needs. A job that reads `needs` here is planned once they are
            // done (`plan::plan_deferred`).
            Self::Job | Self::JobCondition | Self::Call => &["github", "inputs", "vars", "matrix", "strategy", "needs"],
            Self::Runner | Self::StepCondition => &RUNNER_CONTEXTS,
            Self::CallSecrets => &["github", "inputs", "vars", "matrix", "strategy", "needs", "secrets"],
            Self::CallOutputs => &["github", "inputs", "vars", "jobs"],
        }
    }
}

/// Checks the contexts and functions `exprs` use against `scope`.
fn check_exprs<'a>(exprs: impl IntoIterator<Item = &'a Expr>, scope: Scope, path: &str, at: Location) -> Result<(), WorkflowError> {
    for expr in exprs {
        if let Some(name) = expr.contexts().into_iter().find(|name| !scope.contexts().contains(name)) {
            if name == "needs" && !matches!(scope, Scope::Call | Scope::CallSecrets) {
                return Err(error(format!("`{path}`: values that depend on `needs` are not supported by rbit"), at));
            }
            let available = scope.contexts().join(", ");
            return Err(error(format!("`{path}`: the `{name}` context is not available here; it can use {available}"), at));
        }
        for function in expr.functions() {
            if function.is_status() && !matches!(scope, Scope::JobCondition | Scope::StepCondition) {
                return Err(error(format!("`{path}`: `{}()` can only be used in `if`", function.name()), at));
            }
            if function == Function::HashFiles && !matches!(scope, Scope::Runner | Scope::StepCondition) {
                return Err(error(format!("`{path}`: `hashFiles` only works in steps, on the runner"), at));
            }
        }
    }
    Ok(())
}

/// Parses the `${{ }}` in `text` and checks them against `scope`.
fn template(text: &str, scope: Scope, path: &str, at: Location) -> Result<(), WorkflowError> {
    let template = Template::parse(text).map_err(|problem| error(format!("`{path}`: {problem}"), at))?;
    check_exprs(template.exprs(), scope, path, at)
}

/// Parses an `if:` and checks it against `scope`.
fn condition(text: &str, scope: Scope, path: &str, at: Location) -> Result<(), WorkflowError> {
    let condition = Condition::parse(text).map_err(|problem| error(format!("`{path}`: {problem}"), at))?;
    check_exprs([condition.expr()], scope, path, at)
}

/// A string field of `mapping` (any non-null scalar, as written), or `None`.
fn text_field(mapping: &Mapping, key: &str, path: &str) -> Result<Option<String>, WorkflowError> {
    match mapping.get(key) {
        None => Ok(None),
        Some(node) if node.is_null() => Ok(None),
        Some(node) => node.as_text().map(Some).ok_or_else(|| error(format!("`{path}` must be a string"), node.at)),
    }
}

/// A string field that may hold expressions, checked against `scope`.
fn template_field(mapping: &Mapping, key: &str, path: &str, scope: Scope) -> Result<Option<String>, WorkflowError> {
    let value = text_field(mapping, key, path)?;
    if let (Some(text), Some(node)) = (&value, mapping.get(key)) {
        template(text, scope, path, node.at)?;
    }
    Ok(value)
}

/// `true`, `false`, or one expression (`continue-on-error`, `fail-fast`), as text.
fn flag(node: &Node, path: &str, scope: Scope) -> Result<String, WorkflowError> {
    match node.as_scalar().map(|scalar| &scalar.value) {
        Some(ScalarValue::Bool(value)) => Ok(value.to_string()),
        Some(ScalarValue::String(text)) if Template::parse(text).is_ok_and(|template| template.as_single_expr().is_some()) => {
            template(text, scope, path, node.at)?;
            Ok(text.clone())
        }
        _ => Err(error(format!("`{path}` must be true, false or an expression"), node.at)),
    }
}

/// A positive number, or one expression (`timeout-minutes`, `max-parallel`), as written.
fn amount(node: &Node, path: &str, scope: Scope, whole: bool) -> Result<String, WorkflowError> {
    let kind = if whole { "a positive whole number" } else { "a positive number" };
    let refuse = || error(format!("`{path}` must be {kind} or an expression"), node.at);
    match node.as_scalar().map(|scalar| &scalar.value) {
        Some(ScalarValue::Int(value)) if *value > 0 => node.as_text().ok_or_else(refuse),
        #[allow(clippy::float_cmp, reason = "only whole numbers are allowed")]
        Some(ScalarValue::Float(value)) if *value > 0.0 && value.is_finite() && (!whole || value.trunc() == *value) => {
            node.as_text().ok_or_else(refuse)
        }
        Some(ScalarValue::String(text)) if Template::parse(text).is_ok_and(|template| template.as_single_expr().is_some()) => {
            template(text, scope, path, node.at)?;
            Ok(text.clone())
        }
        _ => Err(refuse()),
    }
}

/// An `env` mapping: names without `=` or NUL, scalar values as templates.
pub(crate) fn parse_env(node: &Node, path: &str) -> Result<Env, WorkflowError> {
    if node.is_null() {
        return Ok(Env::new());
    }
    let entries =
        node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a mapping; rbit does not evaluate expressions there"), node.at))?;
    let mut env = Env::new();
    for (key, value) in entries.iter() {
        let name_path = format!("{path}.{}", key.name);
        if key.name.is_empty() || key.name.len() > MAX_NAME_BYTES || key.name.contains(['=', '\0']) {
            return Err(error(format!("`{name_path}`: a variable name has 1 to {MAX_NAME_BYTES} bytes, without `=`"), key.at));
        }
        let text = match &value.value {
            Value::Scalar(_) if value.is_null() => String::new(),
            Value::Scalar(_) => value.as_text().unwrap_or_default(),
            _ => return Err(error(format!("`{name_path}` must be a string, number or boolean"), value.at)),
        };
        template(&text, Scope::Runner, &name_path, value.at)?;
        env.insert(key.name.clone(), text);
    }
    Ok(env)
}

/// `defaults`: only `run.shell` and `run.working-directory`.
fn parse_defaults(node: &Node, path: &str) -> Result<RunDefaults, WorkflowError> {
    if node.is_null() {
        return Ok(RunDefaults::default());
    }
    let config = mapping(node, path)?;
    check_keys(config, path, &["run"], &[])?;
    let Some(run) = config.get("run").filter(|node| !node.is_null()) else {
        return Ok(RunDefaults::default());
    };
    let run_path = format!("{path}.run");
    let run = mapping(run, &run_path)?;
    check_keys(run, &run_path, &["shell", "working-directory"], &[])?;
    let shell = run.get("shell").map(|node| parse_shell(node, &format!("{run_path}.shell"))).transpose()?;
    let working_directory = template_field(run, "working-directory", &format!("{run_path}.working-directory"), Scope::Runner)?;
    Ok(RunDefaults { shell, working_directory })
}

/// A `shell`: one of [`Shell`], or an expression for the runner.
fn parse_shell(node: &Node, path: &str) -> Result<String, WorkflowError> {
    let name = node.as_text().ok_or_else(|| error(format!("`{path}` must be a string"), node.at))?;
    if name.contains("${{") {
        template(&name, Scope::Runner, path, node.at)?;
    } else if Shell::parse(&name).is_none() {
        let supported: Vec<&str> = Shell::ALL.iter().map(|shell| shell.name()).collect();
        let message = if name.contains("{0}") {
            format!("`{path}`: custom shell commands are not supported by rbit yet; use one of {}", supported.join(", "))
        } else {
            format!("`{path}`: `{name}` is not a shell rbit supports ({})", supported.join(", "))
        };
        return Err(error(message, node.at));
    }
    Ok(name)
}

/// `credentials` of a container or service: `username` and `password`, as templates.
fn parse_credentials(node: &Node, path: &str) -> Result<RegistryCredentials, WorkflowError> {
    let config = mapping(node, path)?;
    check_keys(config, path, &["username", "password"], &[])?;
    let field = |key: &str| -> Result<String, WorkflowError> {
        template_field(config, key, &format!("{path}.{key}"), Scope::Runner)?
            .ok_or_else(|| error(format!("`{path}` needs `{key}`"), node.at))
    };
    Ok(RegistryCredentials { username: field("username")?, password: field("password")? })
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

// Actions: workflow_run and repository_dispatch.
fn parse_workflow_run(node: &Node) -> Result<WorkflowRunFilters, WorkflowError> {
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

fn parse_repository_dispatch(node: &Node) -> Result<RepositoryDispatchFilters, WorkflowError> {
    let filters = mapping(node, "on.repository_dispatch")?;
    only_keys(filters, "repository_dispatch", &["types"])?;
    let types = filters.get("types").map(|node| strings(node, "on.repository_dispatch.types")).transpose()?;
    Ok(RepositoryDispatchFilters { types })
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

/// Every permission GitHub documents. rbit honours `contents`, `issues` and
/// `pull-requests` (§11.4) and accepts the others without effect.
const GITHUB_PERMISSIONS: [&str; 15] = [
    "actions",
    "attestations",
    "checks",
    "contents",
    "deployments",
    "discussions",
    "id-token",
    "issues",
    "models",
    "packages",
    "pages",
    "pull-requests",
    "repository-projects",
    "security-events",
    "statuses",
];

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
                    other if GITHUB_PERMISSIONS.contains(&other) => {}
                    other => return Err(error(format!("`{path}`: `{other}` is not a permission"), key.at)),
                }
            }
            Ok(permissions)
        }
        Value::Sequence(_) => Err(error(format!("`{path}` must be read-all, write-all or a mapping"), node.at)),
    }
}

/// Workflow-level (`scope` [`Scope::Run`]) or job-level ([`Scope::Job`]) `concurrency`.
fn parse_concurrency(node: &Node, path: &str, scope: Scope) -> Result<Concurrency, WorkflowError> {
    let concurrency = match &node.value {
        Value::Scalar(_) => {
            let group = node.as_text().ok_or_else(|| error(format!("`{path}` must not be empty"), node.at))?;
            Concurrency { group, cancel_in_progress: false, at: node.at }
        }
        Value::Mapping(config) => {
            check_keys(config, path, &["group", "cancel-in-progress"], &[])?;
            let group = config
                .get("group")
                .and_then(Node::as_text)
                .filter(|group| !group.is_empty())
                .ok_or_else(|| error(format!("`{path}` needs a `group`"), node.at))?;
            let cancel_in_progress = match config.get("cancel-in-progress") {
                None => false,
                Some(value) if value.as_scalar().is_some_and(|scalar| matches!(scalar.value, ScalarValue::String(_))) => {
                    return Err(error(
                        format!("`{path}.cancel-in-progress` must be true or false; rbit does not evaluate expressions there"),
                        value.at,
                    ));
                }
                Some(value) => boolean(value, &format!("{path}.cancel-in-progress"))?,
            };
            Concurrency { group, cancel_in_progress, at: node.at }
        }
        Value::Sequence(_) => return Err(error(format!("`{path}` must be a group name or a mapping"), node.at)),
    };
    template(&concurrency.group, scope, &format!("{path}.group"), concurrency.at)?;
    Ok(concurrency)
}

pub(crate) fn valid_job_id(id: &str) -> bool {
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

/// A job's `needs`, each named once.
fn parse_needs(config: &Mapping, path: &str, at: Location) -> Result<Vec<String>, WorkflowError> {
    let needs = config.get("needs").map(|node| strings(node, &format!("{path}.needs"))).transpose()?.unwrap_or_default();
    if let Some(repeated) = needs.iter().enumerate().find_map(|(index, need)| needs[..index].contains(need).then_some(need)) {
        return Err(error(format!("`{path}.needs` names `{repeated}` twice"), config.get("needs").map_or(at, |node| node.at)));
    }
    Ok(needs)
}

/// A job's `if`, checked.
fn job_condition(config: &Mapping, path: &str) -> Result<Option<String>, WorkflowError> {
    let Some(node) = config.get("if").filter(|node| !node.is_null()) else { return Ok(None) };
    let text = node.as_text().ok_or_else(|| error(format!("`{path}.if` must be a condition"), node.at))?;
    condition(&text, Scope::JobCondition, &format!("{path}.if"), node.at)?;
    Ok(Some(text))
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

// Actions: reusable workflows.
/// `uses`, `with` and `secrets` of a job that calls a workflow.
fn parse_call(config: &Mapping, path: &str) -> Result<JobCall, WorkflowError> {
    let (uses_key, node) = config.entry("uses").expect("checked by the caller");
    let uses = node.as_text().ok_or_else(|| error(format!("`{path}.uses` must name a workflow"), node.at))?;
    let target = call_target(&uses).ok_or_else(|| {
        error(format!("`{path}.uses`: `{uses}` must be `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref`"), uses_key.at)
    })?;
    let mut with = BTreeMap::new();
    if let Some(node) = config.get("with").filter(|node| !node.is_null()) {
        for (key, value) in mapping(node, &format!("{path}.with"))?.iter() {
            let input_path = format!("{path}.with.{}", key.name);
            let text = value.as_text().ok_or_else(|| error(format!("`{input_path}` must be a string, number or boolean"), value.at))?;
            template(&text, Scope::Call, &input_path, value.at)?;
            with.insert(key.name.clone(), text);
        }
    }
    let secrets = match config.get("secrets").filter(|node| !node.is_null()) {
        None => CallSecrets::default(),
        Some(node) if node.as_text().as_deref() == Some("inherit") => CallSecrets::Inherit,
        Some(node) => {
            let mut map = BTreeMap::new();
            for (key, value) in mapping(node, &format!("{path}.secrets"))?.iter() {
                let secret_path = format!("{path}.secrets.{}", key.name);
                let text = value.as_text().ok_or_else(|| error(format!("`{secret_path}` must be a string"), value.at))?;
                template(&text, Scope::CallSecrets, &secret_path, value.at)?;
                map.insert(key.name.clone(), text);
            }
            CallSecrets::Map(map)
        }
    };
    Ok(JobCall { uses, target, with, secrets })
}

/// The workflow `uses` names, or `None` when it has neither form.
pub(crate) fn call_target(uses: &str) -> Option<CallTarget> {
    // Lowercase only, as GitHub: `.github/workflows/ci.YML` is no workflow there.
    #[expect(clippy::case_sensitive_file_extension_comparisons, reason = "GitHub reads only lowercase extensions")]
    let workflow_file = |path: &str| {
        (path.ends_with(".yml") || path.ends_with(".yaml"))
            && path.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'))
    };
    if let Some(path) = uses.strip_prefix("./") {
        return workflow_file(path).then(|| CallTarget::Local { path: path.to_owned() });
    }
    let (spec, reference) = uses.rsplit_once('@')?;
    let mut parts = spec.splitn(3, '/');
    let (owner, repo, path) = (parts.next()?, parts.next()?, parts.next()?);
    let name = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    (name(owner) && name(repo) && workflow_file(path) && !reference.is_empty() && !reference.contains(char::is_whitespace)).then(|| {
        CallTarget::Remote { owner: owner.to_owned(), repo: repo.to_owned(), path: path.to_owned(), reference: reference.to_owned() }
    })
}

/// `on.workflow_call`: `inputs`, `outputs` and `secrets`.
fn parse_workflow_call(node: &Node) -> Result<WorkflowCall, WorkflowError> {
    let config = mapping(node, "on.workflow_call")?;
    only_keys(config, "workflow_call", &["inputs", "outputs", "secrets"])?;
    let mut call = WorkflowCall::default();
    if let Some(inputs) = config.get("inputs").filter(|node| !node.is_null()) {
        for (key, node) in mapping(inputs, "on.workflow_call.inputs")?.iter() {
            let path = format!("on.workflow_call.inputs.{}", key.name);
            let entry = mapping(node, &path)?;
            check_keys(entry, &path, &["description", "required", "default", "type"], &[])?;
            let kind = match entry.get("type").and_then(Node::as_text).as_deref() {
                Some("boolean") => CallInputKind::Boolean,
                Some("number") => CallInputKind::Number,
                Some("string") => CallInputKind::String,
                _ => return Err(error(format!("`{path}.type` must be boolean, number or string"), node.at)),
            };
            let required = match entry.get("required") {
                None => false,
                Some(node) => match node.as_scalar().map(|scalar| &scalar.value) {
                    Some(ScalarValue::Bool(value)) => *value,
                    _ => return Err(error(format!("`{path}.required` must be true or false"), node.at)),
                },
            };
            let default = match entry.get("default").filter(|node| !node.is_null()) {
                None => None,
                Some(node) => {
                    let text = node.as_text().ok_or_else(|| error(format!("`{path}.default` must be a {}", kind.as_str()), node.at))?;
                    Some(
                        call_input_value(kind, &text)
                            .ok_or_else(|| error(format!("`{path}.default` must be a {}", kind.as_str()), node.at))?,
                    )
                }
            };
            call.inputs.insert(key.name.clone(), CallInput { kind, required, default });
        }
    }
    if let Some(outputs) = config.get("outputs").filter(|node| !node.is_null()) {
        for (key, node) in mapping(outputs, "on.workflow_call.outputs")?.iter() {
            let path = format!("on.workflow_call.outputs.{}", key.name);
            let entry = mapping(node, &path)?;
            check_keys(entry, &path, &["description", "value"], &[])?;
            let value = template_field(entry, "value", &format!("{path}.value"), Scope::CallOutputs)?
                .ok_or_else(|| error(format!("`{path}` needs a `value`"), node.at))?;
            call.outputs.insert(key.name.clone(), value);
        }
    }
    if let Some(secrets) = config.get("secrets").filter(|node| !node.is_null()) {
        for (key, node) in mapping(secrets, "on.workflow_call.secrets")?.iter() {
            let path = format!("on.workflow_call.secrets.{}", key.name);
            let required = if node.is_null() {
                false
            } else {
                let entry = mapping(node, &path)?;
                check_keys(entry, &path, &["description", "required"], &[])?;
                match entry.get("required").map(|node| node.as_scalar().map(|scalar| &scalar.value)) {
                    None => false,
                    Some(Some(ScalarValue::Bool(value))) => *value,
                    Some(_) => return Err(error(format!("`{path}.required` must be true or false"), node.at)),
                }
            };
            call.secrets.insert(key.name.clone(), required);
        }
    }
    Ok(call)
}

/// `text` as a value of a `workflow_call` input of `kind`, or `None` when it is not one.
pub fn call_input_value(kind: CallInputKind, text: &str) -> Option<serde_json::Value> {
    match kind {
        CallInputKind::String => Some(serde_json::Value::String(text.to_owned())),
        CallInputKind::Boolean => match text.trim() {
            "true" => Some(serde_json::Value::Bool(true)),
            "false" => Some(serde_json::Value::Bool(false)),
            _ => None,
        },
        CallInputKind::Number => text.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(serde_json::Value::Number),
    }
}

// Actions: environments.
/// `environment`: a name, or `{name, url}`. The name is the server's to evaluate; the URL
/// the runner's, after the steps.
fn parse_environment(node: &Node, path: &str) -> Result<EnvironmentRef, WorkflowError> {
    let (name, url, name_at) = match &node.value {
        Value::Mapping(config) => {
            check_keys(config, path, &["name", "url"], &[])?;
            let name = config.get("name").ok_or_else(|| error(format!("`{path}` needs a `name`"), node.at))?;
            let text = name.as_text().ok_or_else(|| error(format!("`{path}.name` must be a string"), name.at))?;
            (text, template_field(config, "url", &format!("{path}.url"), Scope::Runner)?, name.at)
        }
        _ => (node.as_text().ok_or_else(|| error(format!("`{path}` must be a name or `{{name, url}}`"), node.at))?, None, node.at),
    };
    if name.trim().is_empty() {
        return Err(error(format!("`{path}` must name an environment"), name_at));
    }
    template(&name, Scope::Job, path, name_at)?;
    Ok(EnvironmentRef { name, url })
}

fn parse_runs_on(node: &Node, path: &str) -> Result<Vec<String>, WorkflowError> {
    let labels = match &node.value {
        Value::Mapping(config) => {
            if config.get("group").is_some() {
                return Err(error(format!("`{path}`: runner groups are not supported by rbit; use `labels`"), node.at));
            }
            check_keys(config, path, &["labels"], &[])?;
            config.get("labels").map(|labels| strings(labels, &format!("{path}.labels"))).transpose()?.unwrap_or_default()
        }
        _ => strings(node, path)?,
    };
    if labels.is_empty() || labels.iter().any(String::is_empty) {
        return Err(error(format!("`{path}` must name at least one label, and no empty one"), node.at));
    }
    for label in &labels {
        template(label, Scope::Job, path, node.at)?;
    }
    Ok(labels)
}

/// `outputs`: names as GitHub allows them, values as templates.
fn parse_outputs(node: &Node, path: &str) -> Result<BTreeMap<String, String>, WorkflowError> {
    if node.is_null() {
        return Ok(BTreeMap::new());
    }
    let mut outputs = BTreeMap::new();
    for (key, value) in mapping(node, path)?.iter() {
        let output_path = format!("{path}.{}", key.name);
        let text = value.as_text().ok_or_else(|| error(format!("`{output_path}` must be a string"), value.at))?;
        template(&text, Scope::Runner, &output_path, value.at)?;
        outputs.insert(key.name.clone(), text);
        crate::protocol::validate_outputs(&outputs).map_err(|problem| error(format!("`{path}`: {}", problem.message), key.at))?;
    }
    Ok(outputs)
}

/// `strategy`: `matrix`, `fail-fast` and `max-parallel`.
fn parse_strategy(node: &Node, path: &str) -> Result<Strategy, WorkflowError> {
    if node.is_null() {
        return Ok(Strategy::default());
    }
    let config = mapping(node, path)?;
    check_keys(config, path, &["matrix", "fail-fast", "max-parallel"], &[])?;
    let matrix =
        config.get("matrix").filter(|node| !node.is_null()).map(|node| parse_matrix(node, &format!("{path}.matrix"))).transpose()?;
    let fail_fast = config
        .get("fail-fast")
        .filter(|node| !node.is_null())
        .map(|node| flag(node, &format!("{path}.fail-fast"), Scope::Strategy))
        .transpose()?;
    let max_parallel = config
        .get("max-parallel")
        .filter(|node| !node.is_null())
        .map(|node| amount(node, &format!("{path}.max-parallel"), Scope::Strategy, true))
        .transpose()?;
    Ok(Strategy { matrix, fail_fast, max_parallel })
}

/// `container`: an image, or `image`, `env` and `credentials`.
fn parse_container(node: &Node, path: &str) -> Result<Container, WorkflowError> {
    if let Value::Scalar(_) = &node.value {
        let image =
            node.as_text().filter(|image| !image.is_empty()).ok_or_else(|| error(format!("`{path}` must name an image"), node.at))?;
        template(&image, Scope::Job, path, node.at)?;
        return Ok(Container { image, env: Env::new(), credentials: None, options: None, volumes: Vec::new(), at: node.at });
    }
    let config = mapping(node, path)?;
    check_keys(config, path, &CONTAINER_KEYS, &CONTAINER_UNSUPPORTED)?;
    let (image, env, credentials) = image_env_credentials(config, node, path)?;
    let (options, volumes) = options_volumes(config, path, false)?;
    Ok(Container { image, env, credentials, options, volumes, at: node.at })
}

/// `options` and `volumes` of a container or service, checked now unless they hold
/// expressions ([`crate::options`]).
fn options_volumes(config: &Mapping, path: &str, service: bool) -> Result<(Option<String>, Vec<String>), WorkflowError> {
    let options = template_field(config, "options", &format!("{path}.options"), Scope::Job)?.filter(|text| !text.trim().is_empty());
    if let (Some(text), Some(node)) = (&options, config.get("options"))
        && !text.contains("${{")
    {
        crate::options::docker_options(text, service).map_err(|problem| error(format!("`{path}.options`: {problem}"), node.at))?;
    }
    let mut volumes = Vec::new();
    if let Some(node) = config.get("volumes").filter(|node| !node.is_null()) {
        for volume in strings(node, &format!("{path}.volumes"))? {
            template(&volume, Scope::Job, &format!("{path}.volumes"), node.at)?;
            if !volume.contains("${{") {
                crate::options::volume(&volume).map_err(|problem| error(format!("`{path}.volumes`: {problem}"), node.at))?;
            }
            volumes.push(volume);
        }
    }
    Ok((options, volumes))
}

/// `services`: at most [`MAX_SERVICES`], each `image`, `env` and `credentials`.
fn parse_services(node: &Node, path: &str) -> Result<Vec<Service>, WorkflowError> {
    if node.is_null() {
        return Ok(Vec::new());
    }
    let entries = mapping(node, path)?;
    if entries.len() > MAX_SERVICES {
        return Err(WorkflowError::limit(format!("`{path}`: a job may have at most {MAX_SERVICES} services"), Some(node.at)));
    }
    let mut services = Vec::with_capacity(entries.len());
    for (key, value) in entries.iter() {
        let service_path = format!("{path}.{}", key.name);
        if !valid_job_id(&key.name) {
            return Err(error(
                format!("`{service_path}`: a service id must start with a letter or '_' and contain only letters, digits, '_' and '-'"),
                key.at,
            ));
        }
        let config = mapping(value, &service_path)?;
        check_keys(config, &service_path, &CONTAINER_KEYS, &SERVICE_UNSUPPORTED)?;
        let (image, env, credentials) = image_env_credentials(config, value, &service_path)?;
        let (options, volumes) = options_volumes(config, &service_path, true)?;
        services.push(Service { id: key.name.clone(), image, env, credentials, options, volumes, at: key.at });
    }
    Ok(services)
}

/// The `image` (required), `env` and `credentials` of a container or service.
fn image_env_credentials(config: &Mapping, node: &Node, path: &str) -> Result<(String, Env, Option<RegistryCredentials>), WorkflowError> {
    let image = template_field(config, "image", &format!("{path}.image"), Scope::Job)?
        .filter(|image| !image.is_empty())
        .ok_or_else(|| error(format!("`{path}` needs an `image`"), node.at))?;
    let env = config.get("env").map(|env| parse_env(env, &format!("{path}.env"))).transpose()?.unwrap_or_default();
    let credentials = config
        .get("credentials")
        .filter(|node| !node.is_null())
        .map(|node| parse_credentials(node, &format!("{path}.credentials")))
        .transpose()?;
    Ok((image, env, credentials))
}

/// `steps`: 1 to [`MAX_STEPS`], step ids unique.
pub(crate) fn parse_steps(node: &Node, path: &str) -> Result<Vec<Step>, WorkflowError> {
    let items = node.as_sequence().ok_or_else(|| error(format!("`{path}` must be a list of steps"), node.at))?;
    if items.is_empty() {
        return Err(error(format!("`{path}` must have at least one step"), node.at));
    }
    if items.len() > MAX_STEPS {
        return Err(WorkflowError::limit(format!("`{path}`: a job may have at most {MAX_STEPS} steps"), Some(node.at)));
    }
    let mut steps: Vec<Step> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let step = parse_step(item, &format!("{path}[{index}]"))?;
        if let Some(id) = &step.id
            && steps.iter().any(|other| other.id.as_ref() == Some(id))
        {
            return Err(error(format!("`{path}[{index}]`: step id `{id}` is used twice"), item.at));
        }
        steps.push(step);
    }
    Ok(steps)
}

fn parse_step(node: &Node, path: &str) -> Result<Step, WorkflowError> {
    let config = node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a step: a mapping with `run` or `uses`"), node.at))?;
    check_keys(config, path, &STEP_KEYS, &[])?;
    let field = |key: &str| format!("{path}.{key}");
    let id = text_field(config, "id", &field("id"))?;
    if let Some(id) = &id
        && !valid_job_id(id)
    {
        let at = config.get("id").map_or(node.at, |node| node.at);
        return Err(error(
            format!("`{path}.id`: `{id}` must start with a letter or '_' and contain only letters, digits, '_' and '-'"),
            at,
        ));
    }
    let name = template_field(config, "name", &field("name"), Scope::Runner)?;
    let condition = match config.get("if").filter(|node| !node.is_null()) {
        None => None,
        Some(node) => {
            let text = node.as_text().ok_or_else(|| error(format!("`{path}.if` must be a condition"), node.at))?;
            self::condition(&text, Scope::StepCondition, &field("if"), node.at)?;
            Some(text)
        }
    };
    let action = match (config.entry("run"), config.entry("uses")) {
        (Some(_), Some((key, _))) => return Err(error(format!("`{path}` has both `run` and `uses`; a step does one"), key.at)),
        (None, None) => return Err(error(format!("`{path}` needs `run` or `uses`"), node.at)),
        (Some(_), None) => {
            if let Some((key, _)) = config.entry("with") {
                return Err(error(format!("`{path}.with` only applies to `uses` steps"), key.at));
            }
            StepAction::Run(template_field(config, "run", &field("run"), Scope::Runner)?.unwrap_or_default())
        }
        (None, Some((_, uses_node))) => {
            for key in ["shell", "working-directory"] {
                if let Some((key, _)) = config.entry(key) {
                    return Err(error(format!("`{path}.{}` only applies to `run` steps", key.name), key.at));
                }
            }
            let uses = uses_node
                .as_text()
                .filter(|uses| !uses.is_empty())
                .ok_or_else(|| error(format!("`{path}.uses` must name an action"), uses_node.at))?;
            match builtins::resolve_uses(&uses).map_err(|problem| error(format!("`{path}.uses`: {problem}"), uses_node.at))? {
                builtins::Uses::Builtin(kind, version) => {
                    let check = |key: &str, value: &str| builtins::check_input(kind, &uses, key, value);
                    let with = config.get("with").map(|node| parse_with(node, &field("with"), &check)).transpose()?.unwrap_or_default();
                    builtins::check_required(kind, &uses, &with).map_err(|problem| error(format!("`{path}`: {problem}"), uses_node.at))?;
                    StepAction::Uses(Builtin { uses, kind, version, with })
                }
                // Actions: third-party actions. Inputs are the action's to check, when it is
                // fetched.
                builtins::Uses::Action(source) => {
                    let check = |key: &str, _: &str| {
                        if key.is_empty() || key.len() > 100 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
                            Err(format!("`{}` is not an input name", key.escape_debug()))
                        } else {
                            Ok(())
                        }
                    };
                    let with = config.get("with").map(|node| parse_with(node, &field("with"), &check)).transpose()?.unwrap_or_default();
                    StepAction::Action(ActionStep { uses, source, with })
                }
            }
        }
    };
    let shell = config.get("shell").filter(|node| !node.is_null()).map(|node| parse_shell(node, &field("shell"))).transpose()?;
    let working_directory = template_field(config, "working-directory", &field("working-directory"), Scope::Runner)?;
    let env = config.get("env").map(|node| parse_env(node, &field("env"))).transpose()?.unwrap_or_default();
    let continue_on_error = config
        .get("continue-on-error")
        .filter(|node| !node.is_null())
        .map(|node| flag(node, &field("continue-on-error"), Scope::Runner))
        .transpose()?;
    let timeout_minutes = config
        .get("timeout-minutes")
        .filter(|node| !node.is_null())
        .map(|node| amount(node, &field("timeout-minutes"), Scope::Runner, false))
        .transpose()?;
    Ok(Step { id, name, condition, action, shell, working_directory, env, continue_on_error, timeout_minutes, at: node.at })
}

/// `with`: scalar values, as templates, each passing `check` (the built-in's inputs).
fn parse_with(
    node: &Node,
    path: &str,
    check: &dyn Fn(&str, &str) -> Result<(), String>,
) -> Result<BTreeMap<String, String>, WorkflowError> {
    if node.is_null() {
        return Ok(BTreeMap::new());
    }
    let mut with = BTreeMap::new();
    for (key, value) in mapping(node, path)?.iter() {
        let value_path = format!("{path}.{}", key.name);
        let text = match &value.value {
            Value::Scalar(_) if value.is_null() => String::new(),
            Value::Scalar(_) => value.as_text().unwrap_or_default(),
            _ => return Err(error(format!("`{value_path}` must be a string, number or boolean"), value.at)),
        };
        template(&text, Scope::Runner, &value_path, value.at)?;
        check(&key.name, &text).map_err(|problem| error(format!("`{value_path}`: {problem}"), key.at))?;
        with.insert(key.name.clone(), text);
    }
    Ok(with)
}

/// `strategy.matrix`: a mapping of lists with `include` and `exclude`, where any part may
/// be an expression over `github`, `inputs` and `vars`. What is written out is checked
/// here, with locations; what expressions produce is checked when the job is planned.
fn parse_matrix(node: &Node, path: &str) -> Result<Matrix, WorkflowError> {
    let Value::Mapping(config) = &node.value else {
        let text = node
            .as_text()
            .filter(|text| text.contains("${{"))
            .ok_or_else(|| error(format!("`{path}` must be a mapping of lists, or an expression"), node.at))?;
        template(&text, Scope::Strategy, path, node.at)?;
        return Ok(Matrix { value: serde_json::Value::String(text), has_expressions: true, at: node.at });
    };
    let has_expressions = matrix_expressions(node, path)?;
    let is_expression = |node: &Node| node.as_text().is_some_and(|text| text.contains("${{"));
    let mut dimensions = Vec::new();
    let mut include = false;
    let mut exclude = Vec::new();
    for (key, value) in config.iter() {
        if is_expression(value) {
            include |= key.name == "include";
            if !matches!(key.name.as_str(), "include" | "exclude") {
                dimensions.push(key.name.as_str());
            }
            continue;
        }
        match key.name.as_str() {
            "include" | "exclude" => {
                let items = value
                    .as_sequence()
                    .ok_or_else(|| error(format!("`{path}.{}` must be a list of mappings, or an expression", key.name), value.at))?;
                for item in items {
                    let entry = mapping(item, &format!("{path}.{}[]", key.name))?;
                    if key.name == "exclude" {
                        exclude.push(entry);
                    }
                }
                include |= key.name == "include" && !items.is_empty();
            }
            name => {
                if value.as_sequence().is_none() {
                    return Err(error(format!("`{path}.{name}` must be a list, or an expression"), value.at));
                }
                dimensions.push(name);
            }
        }
    }
    // GitHub refuses an exclude naming a key that is not a dimension (it silently skips
    // such an include key instead).
    for entry in &exclude {
        if let Some((key, _)) = entry.iter().find(|(key, _)| !dimensions.contains(&key.name.as_str())) {
            return Err(error(format!("`{path}.exclude`: `{}` is not a matrix dimension", key.name), key.at));
        }
    }
    if dimensions.is_empty() && !include {
        return Err(error(format!("`{path}` must have a dimension or an `include`"), node.at));
    }
    Ok(Matrix { value: node.to_json(), has_expressions, at: node.at })
}

/// Checks every `${{ }}` in a matrix against [`Scope::Strategy`]; whether there is one.
fn matrix_expressions(node: &Node, path: &str) -> Result<bool, WorkflowError> {
    match &node.value {
        Value::Scalar(scalar) => match &scalar.value {
            ScalarValue::String(text) if text.contains("${{") => template(text, Scope::Strategy, path, node.at).map(|()| true),
            _ => Ok(false),
        },
        Value::Sequence(items) => items.iter().try_fold(false, |found, item| Ok(matrix_expressions(item, path)? || found)),
        Value::Mapping(entries) => entries.iter().try_fold(false, |found, (_, value)| Ok(matrix_expressions(value, path)? || found)),
    }
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
    use crate::spec::BuiltinKind;

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
        let matrix = build.strategy.matrix.as_ref().unwrap();
        assert_eq!(
            matrix.value,
            serde_json::json!({
                "os": ["ubuntu-latest", "debian-12"],
                "rust": ["stable", 1.94],
                "include": [{"os": "debian-12", "experimental": true}],
                "exclude": [{"os": "debian-12", "rust": 1.94}],
            })
        );
        assert!(!matrix.has_expressions);
        assert_eq!(matrix.at, Location::new(34, 9));
        let deploy = workflow.job("deploy").unwrap();
        assert_eq!(deploy.needs, ["build"]);
        assert_eq!(deploy.runs_on, ["self-hosted", "linux"]);
        assert_eq!(deploy.permissions, Some(Permissions::uniform(Permission::Read)));
    }

    #[test]
    fn short_forms_of_on() {
        let workflow = parse_ok("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_none());
        let workflow = parse_ok("on: [push, pull_request, issues, workflow_dispatch]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_some() && workflow.on.workflow_dispatch.is_some());
        assert_eq!(workflow.on.other, ["issues"]);
        let workflow = parse_ok("'on':\n  push:\n  pull_request: {}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
        assert!(workflow.on.push.is_some() && workflow.on.pull_request.as_ref().unwrap().types.is_none());
    }

    #[test]
    fn permissions_forms() {
        let job = |permissions: &str| {
            let source = format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x}}]\n    permissions: {permissions}\n");
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
            ("jobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "no `on` section", (1, 1)),
            ("on: push\n", "no `jobs` section", (1, 1)),
            ("on: push\njobs: {}\n", "at least one job", (2, 1)),
            ("- a\n", "must be a mapping", (1, 1)),
            ("on: push\njobs:\n  1st: {runs-on: x, steps: [{run: x}]}\n", "job id `1st`", (3, 3)),
            ("on: push\njobs:\n  a b: {runs-on: x, steps: [{run: x}]}\n", "job id `a b`", (3, 3)),
            ("on: push\njobs:\n  a: {steps: []}\n", "needs `runs-on`", (3, 3)),
            ("on: push\njobs:\n  a: {runs-on: []}\n", "at least one label", (3, 16)),
            ("on: push\njobs:\n  a: {runs-on: {group: big}}\n", "runner groups", (3, 16)),
            (
                "on: push\njobs:\n  a: {uses: ./x.txt}\n",
                "must be `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref`",
                (3, 7),
            ),
            ("on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: b}\n", "needs `b`, which does not exist", (3, 3)),
            ("on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: a}\n", "needs itself", (3, 3)),
            (
                "on: push\njobs:\n  a: {steps: [{run: x}], runs-on: x, needs: b}\n  b: {steps: [{run: x}], runs-on: x, needs: c}\n  c: {steps: [{run: x}], runs-on: x, needs: a}\n",
                "cycle",
                (3, 3),
            ),
            (
                "on:\n  push:\n    branches: [a]\n    branches-ignore: [b]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "both `branches`",
                (3, 5),
            ),
            ("on:\n  push:\n    branches: ['[]']\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "invalid pattern", (3, 15)),
            ("on:\n  push:\n    types: [created]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "does not support `types`", (3, 5)),
            ("on:\n  pull_request:\n    types: [opend]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`opend` is not", (3, 12)),
            ("on:\n  schedule: '* * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "list of `cron`", (2, 13)),
            ("on:\n  schedule:\n    - crn: x\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`cron` string", (3, 7)),
            ("on:\n  schedule:\n    - cron: '61 * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "between 0 and 59", (3, 13)),
            (
                "on:\n  schedule:\n    - cron: '* * * * *'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "more often than every 5 minutes",
                (3, 13),
            ),
            ("on: push\nconcurrency: {cancel-in-progress: true}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "needs a `group`", (2, 14)),
            (
                "on: push\nconcurrency: {group: g, cancel-in-progress: '${{ x }}'}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "true or false",
                (2, 45),
            ),
            (
                "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: linux}\n",
                "must be a mapping of lists, or an expression",
                (5, 24),
            ),
            (
                "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: ['${{ matrix.x }}']}}\n",
                "the `matrix` context is not available here",
                (5, 30),
            ),
            ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: linux}}\n", "must be a list", (5, 29)),
            (
                "on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {os: [a], exclude: [{arch: x}]}}\n",
                "`arch` is not a matrix dimension",
                (5, 45),
            ),
            ("on: push\njobs:\n  a:\n    runs-on: x\n    strategy: {matrix: {}}\n", "a dimension or an `include`", (5, 24)),
            (
                "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "needs `options`",
                (4, 10),
            ),
            (
                "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: choice, options: [a], default: b}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "one of its options",
                (4, 10),
            ),
            (
                "on:\n  workflow_dispatch:\n    inputs:\n      x: {type: file}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "not an input type",
                (4, 10),
            ),
            (
                "on:\n  workflow_dispatch:\n    inputs:\n      'a b': {}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "input names",
                (4, 7),
            ),
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
        let jobs = (0..=Limits::default().max_jobs)
            .map(|index| format!("  j{index}: {{runs-on: x, steps: [{{run: x}}]}}\n"))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(parse_err(&format!("on: push\njobs:\n{jobs}")).kind, ErrorKind::Limit);
        let inputs = (0..=MAX_DISPATCH_INPUTS).map(|index| format!("      i{index}: {{}}\n")).collect::<Vec<_>>().concat();
        let source = format!("on:\n  workflow_dispatch:\n    inputs:\n{inputs}jobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n");
        assert_eq!(parse_err(&source).kind, ErrorKind::Limit);
        let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
        assert_eq!(parse(CI.as_bytes(), &Limits::default(), &expired).unwrap_err().kind, ErrorKind::Timeout);
    }

    const EVERY_KEY: &str = "\
name: Every key
on: push
env:
  CARGO_TERM_COLOR: always
  LEVEL: 3
  EMPTY:
defaults:
  run:
    shell: bash
    working-directory: crates
jobs:
  test:
    name: Test ${{ matrix.os }}
    needs: [lint]
    if: github.ref == 'refs/heads/main'
    runs-on: [self-hosted, '${{ matrix.os }}']
    permissions: {contents: read}
    env:
      RUST: ${{ matrix.rust }}
    defaults:
      run: {shell: pwsh}
    outputs:
      version: ${{ steps.meta.outputs.version }}
    timeout-minutes: 30
    continue-on-error: ${{ matrix.experimental }}
    concurrency:
      group: test-${{ github.ref }}-${{ matrix.os }}
      cancel-in-progress: true
    strategy:
      fail-fast: false
      max-parallel: 2
      matrix:
        os: [linux, windows]
    container:
      image: docker.io/library/rust:${{ matrix.rust || '1.94' }}
      env: {CARGO_HOME: /cargo}
      credentials:
        username: ci
        password: ${{ secrets.REGISTRY_PASSWORD }}
    services:
      postgres:
        image: postgres:17
        env: {POSTGRES_PASSWORD: pw}
      redis:
        image: redis:7
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          lfs: true
      - id: meta
        name: Meta on ${{ runner.os }}
        if: ${{ always() }}
        run: echo \"version=1\" >> \"$GITHUB_OUTPUT\"
        shell: sh
        working-directory: ${{ github.workspace }}/x
        env: {TOKEN: '${{ secrets.GITHUB_TOKEN }}'}
        continue-on-error: true
        timeout-minutes: ${{ matrix.minutes }}
  lint:
    runs-on: linux
    container: alpine:3
    steps:
      - run: cargo clippy
";

    #[test]
    fn every_key_is_typed() {
        let workflow = parse_ok(EVERY_KEY);
        assert_eq!(
            workflow.env,
            Env::from([("CARGO_TERM_COLOR".into(), "always".into()), ("LEVEL".into(), "3".into()), ("EMPTY".into(), String::new())])
        );
        assert_eq!(workflow.defaults, RunDefaults { shell: Some("bash".into()), working_directory: Some("crates".into()) });
        let test = workflow.job("test").unwrap();
        assert_eq!(test.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
        assert_eq!(test.env, Env::from([("RUST".into(), "${{ matrix.rust }}".into())]));
        assert_eq!(test.defaults, RunDefaults { shell: Some("pwsh".into()), working_directory: None });
        assert_eq!(test.outputs, BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]));
        assert_eq!(test.timeout_minutes.as_deref(), Some("30"));
        assert_eq!(test.continue_on_error.as_deref(), Some("${{ matrix.experimental }}"));
        let concurrency = test.concurrency.as_ref().unwrap();
        assert_eq!((concurrency.group.as_str(), concurrency.cancel_in_progress), ("test-${{ github.ref }}-${{ matrix.os }}", true));
        assert_eq!((test.strategy.fail_fast.as_deref(), test.strategy.max_parallel.as_deref()), (Some("false"), Some("2")));
        assert_eq!(test.strategy.matrix.as_ref().map(|matrix| matrix.value.clone()), Some(serde_json::json!({"os": ["linux", "windows"]})));
        let container = test.container.as_ref().unwrap();
        assert_eq!(container.image, "docker.io/library/rust:${{ matrix.rust || '1.94' }}");
        assert_eq!(container.env, Env::from([("CARGO_HOME".into(), "/cargo".into())]));
        assert_eq!(
            container.credentials,
            Some(RegistryCredentials { username: "ci".into(), password: "${{ secrets.REGISTRY_PASSWORD }}".into() })
        );
        let services: Vec<_> =
            test.services.iter().map(|service| (service.id.as_str(), service.image.as_str(), service.env.len())).collect();
        assert_eq!(services, [("postgres", "postgres:17", 1), ("redis", "redis:7", 0)]);
        assert_eq!(test.steps.len(), 2);
        assert_eq!(
            test.steps[0].action,
            StepAction::Uses(Builtin {
                uses: "actions/checkout@v4".into(),
                kind: BuiltinKind::Checkout,
                version: 1,
                with: BTreeMap::from([("fetch-depth".into(), "0".into()), ("lfs".into(), "true".into())])
            })
        );
        let meta = &test.steps[1];
        assert_eq!(
            (meta.id.as_deref(), meta.name.as_deref(), meta.condition.as_deref()),
            (Some("meta"), Some("Meta on ${{ runner.os }}"), Some("${{ always() }}"))
        );
        assert_eq!(meta.action, StepAction::Run("echo \"version=1\" >> \"$GITHUB_OUTPUT\"".into()));
        assert_eq!((meta.shell.as_deref(), meta.working_directory.as_deref()), (Some("sh"), Some("${{ github.workspace }}/x")));
        assert_eq!(meta.env, Env::from([("TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())]));
        assert_eq!((meta.continue_on_error.as_deref(), meta.timeout_minutes.as_deref()), (Some("true"), Some("${{ matrix.minutes }}")));
        let lint = workflow.job("lint").unwrap();
        assert_eq!(lint.container.as_ref().map(|container| container.image.as_str()), Some("alpine:3"));
        assert_eq!((lint.timeout_minutes.as_ref(), lint.continue_on_error.as_ref(), lint.condition.as_ref()), (None, None, None));
        assert!(lint.services.is_empty() && lint.outputs.is_empty() && lint.strategy.matrix.is_none());
    }

    /// `job` is spliced into a workflow as the body of job `a`, at line 4.
    fn job_err(job: &str) -> WorkflowError {
        parse_err(&format!("on: push\njobs:\n  a:\n{job}"))
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "every level's unknown and unsupported keys, in order")]
    fn unknown_and_unsupported_keys_are_errors() {
        let workflow_level: &[(&str, &str, (u32, u32))] = &[
            ("nmae: x\non: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "a workflow does not support `nmae`", (1, 1)),
            (
                "on: {workflow_call: {inputs: {a: {type: text}}}}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "`on.workflow_call.inputs.a.type` must be boolean, number or string",
                (1, 34),
            ),
            ("on: [push, pushh]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n", "`on.pushh` is not an event", (1, 5)),
            (
                "on: push\ndefaults: {run: {shel: bash}}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "`defaults.run` does not support `shel`",
                (2, 18),
            ),
            (
                "on: push\ndefaults: {shell: bash}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "`defaults` does not support `shell`",
                (2, 12),
            ),
            (
                "on: push\nconcurrency: {group: g, cancel: true}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "does not support `cancel`",
                (2, 25),
            ),
            (
                "on: push\npermissions: {content: read}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n",
                "`content` is not a permission",
                (2, 15),
            ),
        ];
        for (source, message, (line, column)) in workflow_level {
            let error = parse_err(source);
            assert!(error.message.contains(message), "{source}: {error}");
            assert_eq!(error.at, Some(Location { line: *line, column: *column }), "{source}: {error}");
        }
        let job_level: &[(&str, &str, (u32, u32))] = &[
            ("    runs_on: x\n    steps: [{run: x}]\n", "`jobs.a` does not support `runs_on`; it supports name, needs, if", (4, 5)),
            ("    runs-on: x\n    environment: {url: x}\n    steps: [{run: x}]\n", "`jobs.a.environment` needs a `name`", (5, 18)),
            ("    runs-on: x\n    environment: ''\n    steps: [{run: x}]\n", "`jobs.a.environment` must name an environment", (5, 18)),
            (
                "    runs-on: x\n    environment: {name: p, reviewers: x}\n    steps: [{run: x}]\n",
                "`jobs.a.environment` does not support `reviewers`",
                (5, 28),
            ),
            ("    runs-on: x\n    environment: ${{ steps.a.outputs.b }}\n    steps: [{run: x}]\n", "steps", (5, 18)),
            (
                "    uses: o/r/.github/workflows/w.yml@v1\n    runs-on: x\n",
                "`jobs.a.runs-on` is not supported by rbit: the called workflow's jobs",
                (5, 5),
            ),
            (
                "    uses: ./w.yml\n    concurrency: g\n",
                "`jobs.a.concurrency` is not supported by rbit on a job that calls a workflow",
                (5, 5),
            ),
            ("    uses: ./w.yml\n    with: {a: '${{ steps.x.outputs.y }}'}\n", "the `steps` context is not available here", (5, 15)),
            (
                "    runs-on: x\n    secrets: inherit\n    steps: [{run: x}]\n",
                "`jobs.a.secrets` is not supported by rbit: only a job that calls a workflow",
                (5, 5),
            ),
            (
                "    runs-on: x\n    with: {A: b}\n    steps: [{run: x}]\n",
                "`jobs.a.with` is not supported by rbit: only a job that calls a workflow",
                (5, 5),
            ),
            (
                "    runs-on: x\n    strategy: {fail_fast: false}\n    steps: [{run: x}]\n",
                "`jobs.a.strategy` does not support `fail_fast`",
                (5, 16),
            ),
            (
                "    runs-on: x\n    container: {image: a, options: --privileged}\n    steps: [{run: x}]\n",
                "`jobs.a.container.options`: `--privileged` is not an option rbit passes",
                (5, 36),
            ),
            (
                "    runs-on: x\n    container: {image: a, volumes: [/x]}\n    steps: [{run: x}]\n",
                "`jobs.a.container.volumes`: `/x` must be `name:/path`",
                (5, 36),
            ),
            (
                "    runs-on: x\n    container: {image: a, credentials: {user: u}}\n    steps: [{run: x}]\n",
                "`jobs.a.container.credentials` does not support `user`",
                (5, 41),
            ),
            ("    runs-on: x\n    container: {image: a, credentials: {username: u}}\n    steps: [{run: x}]\n", "needs `password`", (5, 40)),
            ("    runs-on: x\n    container: {env: {}}\n    steps: [{run: x}]\n", "`jobs.a.container` needs an `image`", (5, 16)),
            (
                "    runs-on: x\n    services: {db: {image: pg, options: --x}}\n    steps: [{run: x}]\n",
                "`jobs.a.services.db.options`: `--x` is not an option rbit passes",
                (5, 41),
            ),
            (
                "    runs-on: x\n    services: {db: {image: pg, volumes: [/x]}}\n    steps: [{run: x}]\n",
                "`jobs.a.services.db.volumes`: `/x` must be `name:/path`",
                (5, 41),
            ),
            (
                "    runs-on: x\n    services: {db: {image: pg, ports: [5432]}}\n    steps: [{run: x}]\n",
                "`jobs.a.services.db.ports` is not supported by rbit",
                (5, 32),
            ),
            ("    runs-on: x\n    services: {db: {env: {}}}\n    steps: [{run: x}]\n", "`jobs.a.services.db` needs an `image`", (5, 20)),
            ("    runs-on: x\n    services: {1db: {image: pg}}\n    steps: [{run: x}]\n", "a service id must start", (5, 16)),
            (
                "    runs-on: x\n    defaults: {run: {shell: bash, cwd: x}}\n    steps: [{run: x}]\n",
                "`jobs.a.defaults.run` does not support `cwd`",
                (5, 35),
            ),
            ("    runs-on: {labels: [x], size: big}\n    steps: [{run: x}]\n", "`jobs.a.runs-on` does not support `size`", (4, 28)),
            ("    runs-on: x\n    steps:\n      - run: x\n        uses_: y\n", "`jobs.a.steps[0]` does not support `uses_`", (7, 9)),
            ("    runs-on: x\n    env: {A=B: x}\n    steps: [{run: x}]\n", "a variable name", (5, 11)),
            ("    runs-on: x\n    env: '${{ fromJSON(vars.ENV) }}'\n    steps: [{run: x}]\n", "`jobs.a.env` must be a mapping", (5, 10)),
            ("    runs-on: x\n    outputs: {1x: y}\n    steps: [{run: x}]\n", "output name `1x`", (5, 15)),
            (
                "    runs-on: x\n    needs: [b, b]\n    steps: [{run: x}]\n  b: {runs-on: x, steps: [{run: x}]}\n",
                "names `b` twice",
                (5, 12),
            ),
        ];
        assert_errors(job_level.iter().map(|(job, message, at)| (*job, job_err(job), *message, *at)));
    }

    /// Every error has its message and location; all mismatches are reported at once.
    fn assert_errors<'a>(cases: impl Iterator<Item = (&'a str, WorkflowError, &'a str, (u32, u32))>) {
        let mismatches: Vec<String> = cases
            .filter(|(_, error, message, (line, column))| {
                !error.message.contains(message) || error.at != Some(Location { line: *line, column: *column })
            })
            .map(|(source, error, message, at)| format!("{source:?}: {error} (expected {message:?} at {at:?})"))
            .collect();
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    // Actions: third-party actions.
    #[test]
    fn other_actions_are_steps_the_runner_fetches() {
        let workflow = parse_ok(
            "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/setup-node@v4\n        with:\n          node-version: 22\n          cache: ${{ matrix.cache }}\n      - uses: ./.github/actions/build\n      - uses: docker://alpine:3.20\n      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567\n",
        );
        let steps = &workflow.jobs[0].steps;
        let StepAction::Action(node) = &steps[0].action else { panic!("{:?}", steps[0].action) };
        assert_eq!(node.uses, "actions/setup-node@v4");
        assert_eq!(
            node.source,
            crate::spec::ActionSource::Repository {
                owner: "actions".into(),
                repo: "setup-node".into(),
                path: String::new(),
                reference: "v4".into()
            }
        );
        assert_eq!((node.with["node-version"].as_str(), node.with["cache"].as_str()), ("22", "${{ matrix.cache }}"));
        assert!(
            matches!(&steps[1].action, StepAction::Action(local) if local.source == crate::spec::ActionSource::Local { path: ".github/actions/build".into() })
        );
        assert!(
            matches!(&steps[2].action, StepAction::Action(docker) if docker.source == crate::spec::ActionSource::Docker { image: "alpine:3.20".into() })
        );
        assert!(matches!(&steps[3].action, StepAction::Uses(builtin) if builtin.kind == BuiltinKind::Checkout), "a pinned built-in");
    }

    #[test]
    fn steps_do_exactly_one_thing() {
        let cases: &[(&str, &str, (u32, u32))] = &[
            ("    steps:\n      - run: x\n        uses: y\n", "has both `run` and `uses`", (7, 9)),
            ("    steps:\n      - name: nothing\n", "needs `run` or `uses`", (6, 9)),
            ("    steps:\n      - run: x\n        with: {a: b}\n", "steps[0].with` only applies to `uses` steps", (7, 9)),
            ("    steps:\n      - uses: y\n        shell: bash\n", "steps[0].shell` only applies to `run` steps", (7, 9)),
            ("    steps:\n      - uses: y\n        working-directory: d\n", "steps[0].working-directory` only applies", (7, 9)),
            ("    steps:\n      - run: x\n      -\n", "`jobs.a.steps[1]` must be a step", (7, 8)),
            ("    steps:\n      - run: x\n      - x\n", "`jobs.a.steps[1]` must be a step", (7, 9)),
            ("    steps: []\n", "at least one step", (5, 12)),
            ("    steps: x\n", "must be a list of steps", (5, 12)),
            ("    steps:\n      - {id: a, run: x}\n      - {id: a, run: y}\n", "step id `a` is used twice", (7, 9)),
            ("    steps:\n      - {id: 1a, run: x}\n", "`1a` must start with a letter", (6, 14)),
            ("    steps:\n      - {run: x, with: {}}\n", "steps[0].with` only applies", (6, 18)),
            ("    steps:\n      - {uses: rbit/checkout@v1, with: {ref: [1]}}\n", "`jobs.a.steps[0].with.ref` must be a string", (6, 46)),
            (
                "    steps:\n      - {uses: 'octo/ci/.github/workflows/x.yml@v1'}\n",
                "`jobs.a.steps[0].uses`: `octo/ci/.github/workflows/x.yml@v1` is a workflow: call it from a job",
                (6, 16),
            ),
            ("    steps:\n      - {uses: 'octo/repo@--upload-pack=x'}\n", "is not a branch, tag or commit", (6, 16)),
            ("    steps:\n      - {uses: actions/setup-node@v4, with: {'a b': x}}\n", "`a b` is not an input name", (6, 46)),
            (
                "    steps:\n      - {uses: actions/checkout@v4, with: {token: x}}\n",
                "`jobs.a.steps[0].with.token`: `actions/checkout@v4` has no input `token`",
                (6, 44),
            ),
            ("    steps:\n      - {uses: actions/checkout@v4, with: {lfs: maybe}}\n", "`lfs` must be true or false", (6, 44)),
            (
                "    steps:\n      - {uses: actions/cache@v4, with: {path: x}}\n",
                "`jobs.a.steps[0]`: `actions/cache@v4` needs `with.key`",
                (6, 16),
            ),
            ("    steps:\n      - {run: x, env: {A: {b: c}}}\n", "`jobs.a.steps[0].env.A` must be a string", (6, 27)),
            ("    steps:\n      - {run: x, continue-on-error: maybe}\n", "true, false or an expression", (6, 37)),
            ("    steps:\n      - {run: x, continue-on-error: 'x ${{ true }}'}\n", "true, false or an expression", (6, 37)),
            ("    steps:\n      - {run: x, timeout-minutes: 0}\n", "a positive number or an expression", (6, 35)),
            ("    steps:\n      - {run: x, timeout-minutes: soon}\n", "a positive number or an expression", (6, 35)),
            ("    steps:\n      - {run: x, shell: fish}\n", "`fish` is not a shell rbit supports", (6, 25)),
            ("    steps:\n      - {run: x, shell: 'perl {0}'}\n", "custom shell commands are not supported by rbit", (6, 25)),
        ];
        assert_errors(cases.iter().map(|(steps, message, at)| (*steps, job_err(&format!("    runs-on: x\n{steps}")), *message, *at)));
        let steps = (0..=MAX_STEPS).map(|index| format!("      - run: echo {index}\n")).collect::<Vec<_>>().concat();
        let error = job_err(&format!("    runs-on: x\n    steps:\n{steps}"));
        assert_eq!(error.kind, ErrorKind::Limit, "{error}");
        let steps = (0..MAX_STEPS).map(|index| format!("      - run: echo {index}\n")).collect::<Vec<_>>().concat();
        parse_ok(&format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n{steps}"));
        assert!(job_err("    runs-on: x\n").message.contains("`jobs.a` needs `steps`"));
        let services = (0..=MAX_SERVICES).map(|index| format!("      s{index}: {{image: x}}\n")).collect::<Vec<_>>().concat();
        assert_eq!(job_err(&format!("    runs-on: x\n    services:\n{services}    steps: [{{run: x}}]\n")).kind, ErrorKind::Limit);
    }

    #[test]
    fn shells_and_amounts() {
        for shell in Shell::ALL {
            let source = format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x, shell: {}}}]\n", shell.name());
            assert_eq!(parse_ok(&source).jobs[0].steps[0].shell.as_deref(), Some(shell.name()));
            assert!(shell.command().contains("{0}") && shell.extension().starts_with('.'));
        }
        assert_eq!(Shell::Bash.command(), "bash --noprofile --norc -eo pipefail {0}");
        assert_eq!(Shell::Sh.command(), "sh -e {0}");
        let step = |step: &str| parse_ok(&format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{step}]\n")).jobs[0].steps[0].clone();
        assert_eq!(step("{run: x, shell: '${{ matrix.shell }}'}").shell.as_deref(), Some("${{ matrix.shell }}"));
        assert_eq!(step("{run: x, timeout-minutes: 1.5}").timeout_minutes.as_deref(), Some("1.5"));
        assert_eq!(step("{run: x, continue-on-error: false}").continue_on_error.as_deref(), Some("false"));
        assert_eq!(step("{run: x, if: false}").condition.as_deref(), Some("false"));
        assert_eq!(step("{run: x, if: \"contains(github.ref, 'x')\"}").condition.as_deref(), Some("contains(github.ref, 'x')"));
        assert!(matches!(
            step("{uses: rbit/download-artifact@v1}").action,
            StepAction::Uses(Builtin { kind: BuiltinKind::DownloadArtifact, .. })
        ));
        let job = |job: &str| job_err(&format!("    runs-on: x\n{job}    steps: [{{run: x}}]\n"));
        assert!(job("    timeout-minutes: -1\n").message.contains("a positive number"));
        assert!(job("    strategy: {max-parallel: 1.5}\n").message.contains("a positive whole number"));
        assert!(job("    strategy: {max-parallel: 0}\n").message.contains("a positive whole number"));
        assert!(job("    continue-on-error: yes\n").message.contains("true, false or an expression"));
        let source = "on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: ${{ inputs.minutes }}\n    strategy: {max-parallel: '${{ vars.N }}', fail-fast: '${{ inputs.fast }}'}\n    steps: [{run: x}]\n";
        let job = parse_ok(source).jobs.remove(0);
        assert_eq!(job.timeout_minutes.as_deref(), Some("${{ inputs.minutes }}"));
        assert_eq!(
            (job.strategy.max_parallel.as_deref(), job.strategy.fail_fast.as_deref()),
            (Some("${{ vars.N }}"), Some("${{ inputs.fast }}"))
        );
    }

    /// Each field may only use the contexts it is evaluated with (§12.4).
    #[test]
    fn expressions_are_checked_where_they_are_written() {
        let refused: &[(&str, &str)] = &[
            ("run-name: '${{ matrix.os }}'\n", "`run-name`: the `matrix` context is not available here; it can use github, inputs, vars"),
            ("concurrency: '${{ matrix.os }}'\n", "`concurrency.group`: the `matrix` context is not available"),
            ("run-name: '${{ github.ref'\n", "`run-name`: a `${{` is not closed"),
            ("jobs: {a: {runs-on: '${{ secrets.LABEL }}', steps: [{run: x}]}}\n", "`jobs.a.runs-on`: the `secrets` context"),
            ("jobs: {a: {runs-on: x, name: '${{ steps.x.outputs.y }}', steps: [{run: x}]}}\n", "`jobs.a.name`: the `steps` context"),
            ("jobs: {a: {runs-on: x, name: '${{ success() }}', steps: [{run: x}]}}\n", "`success()` can only be used in `if`"),
            ("jobs: {a: {runs-on: x, name: \"${{ hashFiles('x') }}\", steps: [{run: x}]}}\n", "`hashFiles` only works in steps"),
            ("jobs: {a: {runs-on: x, if: env.X, steps: [{run: x}]}}\n", "`jobs.a.if`: the `env` context"),
            (
                "jobs: {a: {runs-on: x, if: '${{ github.ref }} == b', steps: [{run: x}]}}\n",
                "`jobs.a.if`: a condition must be one expression",
            ),
            ("jobs: {a: {runs-on: x, if: 'github.ref ==', steps: [{run: x}]}}\n", "`jobs.a.if`: an expression is incomplete"),
            (
                "jobs: {a: {runs-on: x, timeout-minutes: '${{ env.T }}', steps: [{run: x}]}}\n",
                "`jobs.a.timeout-minutes`: the `env` context",
            ),
            (
                "jobs: {a: {runs-on: x, strategy: {fail-fast: '${{ matrix.x }}'}, steps: [{run: x}]}}\n",
                "`jobs.a.strategy.fail-fast`: the `matrix`",
            ),
            (
                "jobs: {a: {runs-on: x, container: '${{ secrets.IMAGE }}', steps: [{run: x}]}}\n",
                "`jobs.a.container`: the `secrets` context",
            ),
            (
                "jobs: {a: {runs-on: x, concurrency: '${{ steps.a }}', steps: [{run: x}]}}\n",
                "`jobs.a.concurrency.group`: the `steps` context",
            ),
            (
                "jobs: {a: {runs-on: x, env: {A: '${{ failure() }}'}, steps: [{run: x}]}}\n",
                "`jobs.a.env.A`: `failure()` can only be used in `if`",
            ),
            ("jobs: {a: {runs-on: x, steps: [{run: '${{ always() }}'}]}}\n", "`jobs.a.steps[0].run`: `always()` can only be used in `if`"),
            ("jobs: {a: {runs-on: x, steps: [{run: '${{ github..ref }}'}]}}\n", "`jobs.a.steps[0].run`"),
            ("jobs: {a: {runs-on: x, steps: [{run: x, if: 'foo.bar'}]}}\n", "`foo` is not a context"),
        ];
        for (rest, message) in refused {
            let source = format!("on: push\n{rest}");
            let source =
                if source.contains("jobs:") { source } else { format!("{source}jobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n") };
            let error = parse_err(&source);
            assert!(error.message.contains(message), "{source}: {error}");
        }
        let accepted = "on: push
run-name: ${{ github.actor }} on ${{ inputs.target || vars.DEFAULT }}
jobs:
  a:
    runs-on: ${{ fromJSON(vars.RUNNERS)[matrix.os] }}
    if: ${{ needs.b.result == 'success' || failure() }}
    needs: b
    steps:
      - run: echo ${{ secrets.X }} ${{ steps.s.outputs.y }} ${{ runner.os }} ${{ job.status }} ${{ env.A }}
        if: hashFiles('**/Cargo.lock') != '' && !cancelled()
  b:
    runs-on: x
    steps: [{run: x}]
";
        parse_ok(accepted);
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
        use proptest::{prelude::*, strategy::Strategy};

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
