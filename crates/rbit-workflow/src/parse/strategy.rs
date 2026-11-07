//! Strategy and matrix structure with deferred-expression validation.
use super::expressions::{Scope, amount, flag, template};
use super::syntax::{check_keys, error, mapping};
use crate::error::WorkflowError;
use crate::model::{Matrix, Strategy};
use crate::yaml::{Node, ScalarValue, Value};

/// `strategy`: `matrix`, `fail-fast` and `max-parallel`.
pub(super) fn parse_strategy(node: &Node, path: &str) -> Result<Strategy, WorkflowError> {
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

/// `strategy.matrix`: a mapping of lists with `include` and `exclude`, where any part may
/// be an expression over `github`, `inputs` and `vars`. What is written out is checked
/// here, with locations; what expressions produce is checked when the job is planned.
pub(super) fn parse_matrix(node: &Node, path: &str) -> Result<Matrix, WorkflowError> {
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
pub(super) fn matrix_expressions(node: &Node, path: &str) -> Result<bool, WorkflowError> {
    match &node.value {
        Value::Scalar(scalar) => match &scalar.value {
            ScalarValue::String(text) if text.contains("${{") => template(text, Scope::Strategy, path, node.at).map(|()| true),
            _ => Ok(false),
        },
        Value::Sequence(items) => items.iter().try_fold(false, |found, item| Ok(matrix_expressions(item, path)? || found)),
        Value::Mapping(entries) => entries.iter().try_fold(false, |found, (_, value)| Ok(matrix_expressions(value, path)? || found)),
    }
}
