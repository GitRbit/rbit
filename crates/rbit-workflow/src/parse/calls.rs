//! Reusable workflow targets, input/output declarations and call syntax.
use super::call_target;
use super::expressions::{Scope, template, template_field};
use super::syntax::{check_keys, error, mapping};
use super::triggers::only_keys;
use crate::error::WorkflowError;
use crate::model::{CallInput, CallInputKind, CallSecrets, JobCall, WorkflowCall};
use crate::yaml::{Mapping, Node, ScalarValue};
use std::collections::BTreeMap;

// Actions: reusable workflows.
/// The keys of a job that calls a workflow (`uses`), as on GitHub.
pub(super) const CALL_KEYS: [&str; 8] = ["name", "uses", "with", "secrets", "strategy", "needs", "if", "permissions"];

/// Keys of other jobs that a calling job cannot have, and why.
pub(super) const CALL_UNSUPPORTED: [(&str, &str); 12] = [
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

// Actions: reusable workflows.
/// `uses`, `with` and `secrets` of a job that calls a workflow.
pub(super) fn parse_call(config: &Mapping, path: &str) -> Result<JobCall, WorkflowError> {
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

/// `on.workflow_call`: `inputs`, `outputs` and `secrets`.
pub(super) fn parse_workflow_call(node: &Node) -> Result<WorkflowCall, WorkflowError> {
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
