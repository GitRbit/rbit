//! Step limits, action selection, builtin inputs and runner-scoped expressions.
use super::environment::{parse_env, parse_shell};
use super::expressions::{Scope, amount, condition, flag, template, template_field};
use super::jobs::valid_job_id;
use super::syntax::{check_keys, error, mapping, text_field};
use crate::builtins;
use crate::error::WorkflowError;
use crate::model::{ActionStep, Builtin, Step, StepAction};
use crate::spec::MAX_STEPS;
use crate::yaml::{Node, Value};
use std::collections::BTreeMap;

/// The keys of a step.
pub(super) const STEP_KEYS: [&str; 11] =
    ["id", "name", "if", "run", "shell", "working-directory", "env", "uses", "with", "continue-on-error", "timeout-minutes"];

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

pub(super) fn parse_step(node: &Node, path: &str) -> Result<Step, WorkflowError> {
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
pub(super) fn parse_with(
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
