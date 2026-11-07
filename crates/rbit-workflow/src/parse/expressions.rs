//! Expression contexts, allowed functions and scalar expression validation.
use super::syntax::{error, text_field};
use crate::error::{Location, WorkflowError};
use crate::expr::{Condition, Expr, Function, Template};
use crate::yaml::{Mapping, Node, ScalarValue};

/// Where a field is evaluated (§12.4), which decides the contexts and functions it may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
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
pub(super) const RUNNER_CONTEXTS: [&str; 11] =
    ["github", "env", "vars", "job", "steps", "runner", "secrets", "strategy", "matrix", "needs", "inputs"];

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
pub(super) fn check_exprs<'a>(
    exprs: impl IntoIterator<Item = &'a Expr>,
    scope: Scope,
    path: &str,
    at: Location,
) -> Result<(), WorkflowError> {
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
pub(super) fn template(text: &str, scope: Scope, path: &str, at: Location) -> Result<(), WorkflowError> {
    let template = Template::parse(text).map_err(|problem| error(format!("`{path}`: {problem}"), at))?;
    check_exprs(template.exprs(), scope, path, at)
}

/// Parses an `if:` and checks it against `scope`.
pub(super) fn condition(text: &str, scope: Scope, path: &str, at: Location) -> Result<(), WorkflowError> {
    let condition = Condition::parse(text).map_err(|problem| error(format!("`{path}`: {problem}"), at))?;
    check_exprs([condition.expr()], scope, path, at)
}

/// A string field that may hold expressions, checked against `scope`.
pub(super) fn template_field(mapping: &Mapping, key: &str, path: &str, scope: Scope) -> Result<Option<String>, WorkflowError> {
    let value = text_field(mapping, key, path)?;
    if let (Some(text), Some(node)) = (&value, mapping.get(key)) {
        template(text, scope, path, node.at)?;
    }
    Ok(value)
}

/// `true`, `false`, or one expression (`continue-on-error`, `fail-fast`), as text.
pub(super) fn flag(node: &Node, path: &str, scope: Scope) -> Result<String, WorkflowError> {
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
pub(super) fn amount(node: &Node, path: &str, scope: Scope, whole: bool) -> Result<String, WorkflowError> {
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
