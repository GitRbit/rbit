//! Reusable-workflow permission ceilings, typed inputs and passed secrets.
use super::evaluation::Evaluator;
use crate::error::{Location, WorkflowError};
use crate::expr::Template;
use crate::model::{CallInputKind, CallSecrets, Permission, Permissions};
use crate::parse::call_input_value;
use crate::spec::{DeferredInput, PassedSecrets};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The secrets a call passes, checked against those the called workflow declares.
pub(super) fn passed_secrets(
    evaluator: &Evaluator<'_>,
    secrets: &CallSecrets,
    declared: &crate::model::WorkflowCall,
) -> Result<PassedSecrets, WorkflowError> {
    match secrets {
        CallSecrets::Inherit => Ok(PassedSecrets::Inherit),
        CallSecrets::Map(map) => {
            if let Some(name) = map.keys().find(|name| !declared.secrets.contains_key(*name)) {
                return Err(evaluator.refuse(&format!("secrets.{name}"), &"the called workflow has no such secret"));
            }
            if let Some(name) = declared.secrets.iter().find_map(|(name, required)| (*required && !map.contains_key(name)).then_some(name))
            {
                return Err(evaluator.refuse("secrets", &format!("the called workflow needs the secret `{name}`")));
            }
            Ok(PassedSecrets::Map(map.clone()))
        }
    }
}

/// `declared` permissions within `cap`: what a job gets (the cap when it declares none). A
/// declaration past the cap fails planning, as on GitHub.
pub(super) fn narrowed(
    declared: Option<Permissions>,
    cap: Option<Permissions>,
    job: &str,
    at: Location,
) -> Result<Option<Permissions>, WorkflowError> {
    let (Some(declared), Some(cap)) = (declared, cap) else { return Ok(declared.or(cap)) };
    let rank = |permission: Permission| match permission {
        Permission::None => 0,
        Permission::Read => 1,
        Permission::Write => 2,
    };
    let name = |permission: Permission| match permission {
        Permission::None => "none",
        Permission::Read => "read",
        Permission::Write => "write",
    };
    for (field, asked, allowed) in [
        ("contents", declared.contents, cap.contents),
        ("issues", declared.issues, cap.issues),
        ("pull-requests", declared.pull_requests, cap.pull_requests),
    ] {
        if rank(asked) > rank(allowed) {
            return Err(WorkflowError::plan(
                format!(
                    "job `{job}` asks for `{field}: {}`, but the workflow calling it allows only `{field}: {}`",
                    name(asked),
                    name(allowed)
                ),
                Some(at),
            ));
        }
    }
    Ok(Some(declared))
}

/// The called workflow's inputs: typed values known now, and the `with:` that read `needs`.
/// The `inputs` context of a called workflow, and the inputs only its jobs' runner can
/// evaluate.
type CallInputs = (Map<String, Value>, BTreeMap<String, DeferredInput>);

pub(super) fn call_inputs(
    evaluator: &Evaluator<'_>,
    with: &BTreeMap<String, String>,
    declared: &crate::model::WorkflowCall,
) -> Result<CallInputs, WorkflowError> {
    if let Some(name) = with.keys().find(|name| !declared.inputs.contains_key(*name)) {
        return Err(evaluator.refuse(&format!("with.{name}"), &"the called workflow has no such input"));
    }
    let (mut inputs, mut deferred) = (Map::new(), BTreeMap::new());
    for (name, input) in &declared.inputs {
        let field = format!("with.{name}");
        match with.get(name) {
            Some(text) => {
                let template = Template::parse(text).map_err(|problem| evaluator.refuse(&field, &problem))?;
                if template.exprs().any(|expr| expr.contexts().contains(&"needs")) {
                    deferred.insert(name.clone(), DeferredInput { value: text.clone(), kind: input.kind.as_str().to_owned() });
                } else {
                    let value = evaluator.text(&field, text)?;
                    let typed = call_input_value(input.kind, &value)
                        .ok_or_else(|| evaluator.refuse(&field, &format!("`{value}` is not a {}", input.kind.as_str())))?;
                    inputs.insert(name.clone(), typed);
                }
            }
            None => match (&input.default, input.required) {
                (Some(default), _) => {
                    inputs.insert(name.clone(), default.clone());
                }
                (None, true) => return Err(evaluator.refuse("with", &format!("the called workflow needs the input `{name}`"))),
                (None, false) => {
                    let empty = match input.kind {
                        CallInputKind::Boolean => Value::Bool(false),
                        CallInputKind::Number => json!(0),
                        CallInputKind::String => Value::String(String::new()),
                    };
                    inputs.insert(name.clone(), empty);
                }
            },
        }
    }
    Ok((inputs, deferred))
}
