//! Conformance verdict with unchanged verdict and fixture semantics.
use serde_json::Value;

/// Whether `actual` satisfies `expect`. `{"error": true}` matches any error; a `jobs` list
/// matches its planned jobs in any order, each by subset; everything else is equal, with
/// numbers compared as numbers.
pub(super) fn verdict(expect: &Value, actual: &Value) -> bool {
    if expect.as_object().is_some_and(|object| object.len() == 1) && expect.get("error") == Some(&Value::Bool(true)) {
        return actual.get("error").is_some();
    }
    if let (Some(expected), Some(got)) = (expect.get("jobs").and_then(Value::as_array), actual.get("jobs").and_then(Value::as_array)) {
        return expected.len() == got.len() && jobs_match(expected, got);
    }
    json_eq(expect, actual)
}

/// A planned job matches a listed one when every listed field is present and equal, and its
/// `matrix` is exactly the listed combination: an extra matrix key is a different job.
fn jobs_match(expected: &[Value], actual: &[Value]) -> bool {
    let mut used = vec![false; actual.len()];
    let matches = |wanted: &Value, job: &Value| {
        subset(wanted, job) && wanted.get("matrix").is_none_or(|matrix| json_eq(matrix, job.get("matrix").unwrap_or(&Value::Null)))
    };
    expected.iter().all(|wanted| match actual.iter().enumerate().find(|(index, job)| !used[*index] && matches(wanted, job)) {
        Some((index, _)) => {
            used[index] = true;
            true
        }
        None => false,
    })
}

/// Every key of `expected` appears in `actual` with a matching value; a missing key reads as
/// `null`.
fn subset(expected: &Value, actual: &Value) -> bool {
    match expected {
        Value::Object(object) => object.iter().all(|(key, value)| subset(value, actual.get(key).unwrap_or(&Value::Null))),
        _ => json_eq(expected, actual),
    }
}

fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(key, value)| y.get(key).is_some_and(|other| json_eq(value, other)))
        }
        _ => a == b,
    }
}
