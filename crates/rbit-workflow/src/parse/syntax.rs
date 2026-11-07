//! Located scalar shapes, unknown-key diagnostics and optional text.
use crate::error::{Location, WorkflowError};
use crate::yaml::{Mapping, Node, ScalarValue, Value};

pub(super) fn error(message: impl Into<String>, at: Location) -> WorkflowError {
    WorkflowError::parse(message, at)
}

/// Refuses every key of `mapping` that is not `allowed`; a key of `unsupported` is a GitHub
/// key rbit lacks, with the reason.
pub(super) fn check_keys(mapping: &Mapping, path: &str, allowed: &[&str], unsupported: &[(&str, &str)]) -> Result<(), WorkflowError> {
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

/// A string field of `mapping` (any non-null scalar, as written), or `None`.
pub(super) fn text_field(mapping: &Mapping, key: &str, path: &str) -> Result<Option<String>, WorkflowError> {
    match mapping.get(key) {
        None => Ok(None),
        Some(node) if node.is_null() => Ok(None),
        Some(node) => node.as_text().map(Some).ok_or_else(|| error(format!("`{path}` must be a string"), node.at)),
    }
}

pub(super) fn optional_text(mapping: &Mapping, key: &str) -> Result<Option<String>, WorkflowError> {
    match mapping.get(key) {
        None => Ok(None),
        Some(node) if node.is_null() => Ok(None),
        Some(node) => node.as_text().map(Some).ok_or_else(|| error(format!("`{key}` must be a string"), node.at)),
    }
}

pub(super) fn mapping<'a>(node: &'a Node, path: &str) -> Result<&'a Mapping, WorkflowError> {
    node.as_mapping().ok_or_else(|| error(format!("`{path}` must be a mapping"), node.at))
}

/// A string, or a list of strings (GitHub accepts either for most lists).
pub(super) fn strings(node: &Node, path: &str) -> Result<Vec<String>, WorkflowError> {
    let expected = || error(format!("`{path}` must be a string or a list of strings"), node.at);
    match &node.value {
        Value::Scalar(scalar) if scalar.value != ScalarValue::Null => Ok(vec![scalar.text()]),
        Value::Sequence(items) => items.iter().map(|item| item.as_text().ok_or_else(expected)).collect(),
        _ => Err(expected()),
    }
}

pub(super) fn boolean(node: &Node, path: &str) -> Result<bool, WorkflowError> {
    match node.as_scalar().map(|scalar| &scalar.value) {
        Some(ScalarValue::Bool(value)) => Ok(*value),
        _ => Err(error(format!("`{path}` must be true or false"), node.at)),
    }
}
