//! Manual workflow input declarations and their bounded defaults.
use super::syntax::{boolean, error, mapping, optional_text, strings};
use super::triggers::only_keys;
use crate::error::WorkflowError;
use crate::model::{Dispatch, DispatchInput, InputKind};
use crate::yaml::{Key, Mapping, Node};

/// Most `workflow_dispatch` inputs.
pub const MAX_DISPATCH_INPUTS: usize = 25;

pub(super) fn parse_dispatch(node: &Node) -> Result<Dispatch, WorkflowError> {
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

pub(super) fn parse_input(key: &Key, node: &Node) -> Result<DispatchInput, WorkflowError> {
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
