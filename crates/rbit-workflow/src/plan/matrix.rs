//! Resolve, validate, expand and order matrix combinations in Forgejo-compatible order.
use crate::error::{Location, WorkflowError};
use crate::expr::{Contexts, Template};
use crate::limits::{Deadline, Limits};
use crate::model::Matrix;
use serde_json::{Map, Value};

/// One matrix combination: each key and its value, keys in sorted order.
pub type Combination = Map<String, Value>;

/// A matrix with its expressions evaluated and its shape checked, ready to expand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Axes {
    /// Each dimension and its values.
    pub dimensions: Vec<(String, Vec<Value>)>,
    pub include: Vec<Combination>,
    pub exclude: Vec<Combination>,
    /// Where the matrix is, for errors.
    pub at: Option<Location>,
}

impl Axes {
    /// Evaluates the expressions of `matrix` with the run's contexts and checks the result.
    pub fn resolve(matrix: &Matrix, contexts: &dyn Contexts, job_id: &str) -> Result<Self, WorkflowError> {
        let refuse =
            |problem: &dyn std::fmt::Display| WorkflowError::plan(format!("job `{job_id}`: `strategy.matrix`: {problem}"), Some(matrix.at));
        let value = if matrix.has_expressions {
            evaluate_strings(&matrix.value, contexts).map_err(|problem| refuse(&problem))?
        } else {
            matrix.value.clone()
        };
        let mut axes = Self::from_json(&value).map_err(|problem| refuse(&problem))?;
        axes.at = Some(matrix.at);
        Ok(axes)
    }

    /// The matrix in `value`: an object of lists, with `include` and `exclude` lists of
    /// objects whose `exclude` keys are dimensions, and a dimension or an `include`.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let Value::Object(entries) = value else {
            return Err("must be a mapping of lists".to_owned());
        };
        let mut axes = Self::default();
        for (key, value) in entries {
            let Value::Array(items) = value else {
                return Err(format!("`{key}` must be a list"));
            };
            match key.as_str() {
                "include" | "exclude" => {
                    let entries = items
                        .iter()
                        .map(|item| item.as_object().cloned().ok_or_else(|| format!("`{key}` must be a list of mappings")))
                        .collect::<Result<Vec<_>, _>>()?;
                    if key == "include" {
                        axes.include = entries;
                    } else {
                        axes.exclude = entries;
                    }
                }
                _ => axes.dimensions.push((key.clone(), items.clone())),
            }
        }
        for entry in &axes.exclude {
            if let Some(key) = entry.keys().find(|key| !axes.dimensions.iter().any(|(name, _)| name == *key)) {
                return Err(format!("`exclude`: `{key}` is not a matrix dimension"));
            }
        }
        if axes.dimensions.is_empty() && axes.include.is_empty() {
            return Err("must have a dimension or an `include`".to_owned());
        }
        Ok(axes)
    }
}

/// `value` with every string holding `${{ }}` replaced by its value.
fn evaluate_strings(value: &Value, contexts: &dyn Contexts) -> Result<Value, crate::expr::ExprError> {
    Ok(match value {
        Value::String(text) if text.contains("${{") => Template::parse(text)?.evaluate(contexts)?,
        Value::Array(items) => Value::Array(items.iter().map(|item| evaluate_strings(item, contexts)).collect::<Result<_, _>>()?),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), evaluate_strings(value, contexts)?)))
                .collect::<Result<_, crate::expr::ExprError>>()?,
        ),
        other => other.clone(),
    })
}

/// The combinations of job `job_id`'s `strategy.matrix`, resolved with `contexts`. A matrix
/// that leaves no combination (a dimension without values and no `include`, or an `exclude`
/// that removes them all) is an error, as on GitHub: it never runs one job without a matrix.
pub fn expand(
    matrix: &Matrix,
    contexts: &dyn Contexts,
    job_id: &str,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Vec<Combination>, WorkflowError> {
    let axes = Axes::resolve(matrix, contexts, job_id)?;
    let found = combinations(&axes, limits, deadline)?;
    if found.is_empty() {
        let why = match axes.dimensions.iter().find(|(_, values)| values.is_empty()) {
            Some((key, _)) => format!("`{key}` has no values"),
            None => "`exclude` removes every combination".to_owned(),
        };
        let message = format!("job `{job_id}`: `strategy.matrix`: {why}, so the job has nothing to run");
        return Err(WorkflowError::plan(message, Some(matrix.at)));
    }
    Ok(found)
}

/// Expands a matrix within `limits` (see the module documentation for the rules); an empty
/// list when nothing is left (see [`expand`]).
pub fn combinations(axes: &Axes, limits: &Limits, deadline: &Deadline) -> Result<Vec<Combination>, WorkflowError> {
    let too_many = || WorkflowError::limit(format!("a matrix may have at most {} combinations", limits.max_combinations), axes.at);
    let product = axes.dimensions.iter().try_fold(1usize, |count, (_, values)| count.checked_mul(values.len()));
    if product.is_none_or(|count| count > limits.max_combinations) {
        return Err(too_many());
    }
    let mut combinations: Vec<Combination> = if axes.dimensions.is_empty() { Vec::new() } else { vec![Combination::new()] };
    for (key, values) in &axes.dimensions {
        deadline.check()?;
        combinations = combinations
            .into_iter()
            .flat_map(|combination| {
                values.iter().map(move |value| {
                    let mut next = combination.clone();
                    next.insert(key.clone(), value.clone());
                    next
                })
            })
            .collect();
    }
    combinations.retain(|combination| {
        !axes.exclude.iter().any(|exclude| combination.iter().all(|(key, value)| exclude.get(key).is_none_or(|excluded| excluded == value)))
    });
    let original = |key: &str| axes.dimensions.iter().any(|(name, _)| name == key);
    let mut extra = Vec::new();
    for include in &axes.include {
        deadline.check()?;
        let mut matched = false;
        for combination in &mut combinations {
            let fits = combination.iter().all(|(key, value)| !original(key) || include.get(key).is_none_or(|included| included == value));
            if fits {
                matched = true;
                combination.extend(include.iter().map(|(key, value)| (key.clone(), value.clone())));
            }
        }
        if !matched {
            extra.push(include.clone());
        }
    }
    combinations.extend(extra);
    if combinations.len() > limits.max_combinations {
        return Err(too_many());
    }
    combinations.sort_by_cached_key(matrix_name);
    Ok(combinations)
}

/// `(value, value)`, values in key order, formatted as Go's `fmt.Sprint` formats them.
pub(super) fn matrix_name(combination: &Combination) -> String {
    let values: Vec<String> = combination.values().map(go_format).collect();
    format!("({})", values.join(", "))
}

/// A value as Go's `fmt.Sprint` prints it once decoded into `interface{}`.
pub(super) fn go_format(value: &Value) -> String {
    match value {
        Value::Null => "<nil>".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(integer), _) => integer.to_string(),
            (_, Some(integer)) => integer.to_string(),
            _ => go_float(number.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(text) => text.clone(),
        Value::Array(items) => format!("[{}]", items.iter().map(go_format).collect::<Vec<_>>().join(" ")),
        Value::Object(entries) => {
            format!("map[{}]", entries.iter().map(|(key, value)| format!("{key}:{}", go_format(value))).collect::<Vec<_>>().join(" "))
        }
    }
}

/// Go's `%v` for a float64: the shortest representation, with an exponent (`1e+21`,
/// `1e-05`) below 1e-4 and from 1e21.
pub(super) fn go_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf".to_owned() } else { "-Inf".to_owned() };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-4..1e21).contains(&magnitude) {
        let formatted = format!("{value:e}");
        let (mantissa, exponent) = formatted.split_once('e').unwrap_or((&formatted, "0"));
        let (sign, digits) = exponent.strip_prefix('-').map_or(("+", exponent), |digits| ("-", digits));
        return format!("{mantissa}e{sign}{digits:0>2}");
    }
    format!("{value}")
}
