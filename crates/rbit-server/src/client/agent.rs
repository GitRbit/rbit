//! The client's output contract for programs (`docs/cli.md`, "Output for programs").
//!
//! * With `--json` a command prints exactly one compact JSON object on stdout whose
//!   `schema` names its shape and version (`rbit.runs/1`); progress goes to stderr.
//! * A failure with `--json` is also one object on stdout,
//!   `{"schema":"rbit.error/1","code":"…","message":"…","hint":"…"}`, where `code` is the
//!   server's (an API error) or the client's own (`pending`, `timeout`, `usage`, …).
//! * The exit code says what kind of answer it was, the same with or without `--json`:
//!   0 success or green, 1 failed or red, 2 usage, 3 not signed in or not allowed, 4 not
//!   found, 5 a precondition failed (the head or base moved, a conflict), 8 pending or
//!   timed out waiting, 9 the answer cannot be trusted (a log cut short, the server
//!   unavailable).

use std::fmt;

use serde_json::{Map, Value, json};

/// A request the server refused, with the `code` its body named (or one its status
/// implies, for a server that names none).
#[derive(Debug)]
pub struct ApiFailure {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.status)
    }
}

impl std::error::Error for ApiFailure {}

impl ApiFailure {
    /// From a refused answer's status and body.
    pub fn from_response(status: u16, body: &str) -> Self {
        let parsed = serde_json::from_str::<Value>(body).ok();
        let field = |key: &str| parsed.as_ref().and_then(|value| value[key].as_str()).map(str::to_owned);
        let code = field("code").unwrap_or_else(|| implied_code(status).to_owned());
        let message = match (status, field("message")) {
            (401, _) => "not signed in, or the token is not valid: run `rbit auth login`".to_owned(),
            (_, Some(message)) => message,
            _ => format!("the server answered {status}"),
        };
        Self { status, code, message }
    }
}

/// The code a status implies when the body names none (an older server).
fn implied_code(status: u16) -> &'static str {
    match status {
        401 => "unauthenticated",
        403 => "forbidden",
        404 | 410 => "not_found",
        405 => "blocked",
        409 | 412 => "conflict",
        429 => "rate_limited",
        400..=499 => "invalid",
        503 => "unavailable",
        _ => "internal",
    }
}

/// A result the client itself reports with a code: nothing to show yet, a wait that ran
/// out, a precondition that does not hold.
#[derive(Debug)]
pub struct Outcome {
    pub code: &'static str,
    pub message: String,
    /// A command that can be run as it is to get further.
    pub hint: Option<String>,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.hint {
            Some(hint) => write!(f, "{} (next: {hint})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Outcome {}

/// An [`Outcome`] as an error.
pub fn outcome(code: &'static str, message: impl Into<String>, hint: Option<String>) -> anyhow::Error {
    Outcome { code, message: message.into(), hint }.into()
}

/// The code of `error`: the server's, the client's own, or `error`.
pub fn code_of(error: &anyhow::Error) -> &str {
    if let Some(failure) = error.downcast_ref::<ApiFailure>() {
        &failure.code
    } else if let Some(outcome) = error.downcast_ref::<Outcome>() {
        outcome.code
    } else if error.downcast_ref::<reqwest::Error>().is_some() {
        "unavailable"
    } else {
        "error"
    }
}

/// The exit code of a command that failed with `error`.
pub fn exit_code(error: &anyhow::Error) -> i32 {
    match code_of(error) {
        "usage" | "invalid" => 2,
        "unauthenticated" | "forbidden" => 3,
        "not_found" => 4,
        "head_changed" | "base_changed" | "conflict" => 5,
        "pending" | "timeout" | "no_runner" => 8,
        "log_incomplete" | "unavailable" | "internal" | "rate_limited" | "indeterminate" => 9,
        _ => 1,
    }
}

/// Prints `body` as one compact JSON object with `schema` first.
pub fn print(schema: &str, body: Value) {
    let mut object = Map::new();
    object.insert("schema".to_owned(), Value::String(schema.to_owned()));
    match body {
        Value::Object(fields) => object.extend(fields),
        other => {
            object.insert("value".to_owned(), other);
        }
    }
    println!("{}", Value::Object(object));
}

/// The error object of `error`.
pub fn error_body(error: &anyhow::Error) -> Value {
    let mut body = json!({ "schema": "rbit.error/1", "code": code_of(error), "message": format!("{error:#}") });
    if let Some(outcome) = error.downcast_ref::<Outcome>() {
        body["message"] = Value::String(outcome.message.clone());
        if let Some(hint) = &outcome.hint {
            body["hint"] = Value::String(hint.clone());
        }
    }
    body
}

/// Keeps only `fields` (when any) of each object in `rows`.
pub fn select(rows: Vec<Value>, fields: &[String]) -> Vec<Value> {
    if fields.is_empty() {
        return rows;
    }
    rows.into_iter()
        .map(|row| {
            let kept: Map<String, Value> =
                fields.iter().filter_map(|field| row.get(field).map(|value| (field.clone(), value.clone()))).collect();
            Value::Object(kept)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_keep_the_servers_code_or_imply_one() {
        let failure = ApiFailure::from_response(409, r#"{"message":"Head branch was modified.","code":"head_changed"}"#);
        assert_eq!((failure.code.as_str(), failure.message.as_str()), ("head_changed", "Head branch was modified."));
        assert_eq!(ApiFailure::from_response(404, r#"{"message":"Not Found"}"#).code, "not_found");
        assert_eq!(ApiFailure::from_response(502, "<html>").code, "internal");
        assert!(ApiFailure::from_response(401, "{}").message.contains("rbit auth login"));
    }

    #[test]
    fn exit_codes_follow_the_contract() {
        let api = |status, code: &str| anyhow::Error::from(ApiFailure { status, code: code.to_owned(), message: String::new() });
        assert_eq!(exit_code(&api(404, "not_found")), 4);
        assert_eq!(exit_code(&api(401, "unauthenticated")), 3);
        assert_eq!(exit_code(&api(409, "head_changed")), 5);
        assert_eq!(exit_code(&api(422, "invalid")), 2);
        assert_eq!(exit_code(&api(503, "unavailable")), 9);
        assert_eq!(exit_code(&outcome("pending", "no run yet", None)), 8);
        assert_eq!(exit_code(&outcome("no_runner", "no runner has `gpu`", None)), 8);
        assert_eq!(exit_code(&outcome("log_incomplete", "cut", None)), 9);
        assert_eq!(exit_code(&anyhow::anyhow!("anything else")), 1);
    }

    #[test]
    fn error_objects_name_their_code_and_hint() {
        let body = error_body(&outcome("pending", "no run for abc yet", Some("rbit run watch --sha abc".to_owned())));
        assert_eq!(
            body,
            json!({ "schema": "rbit.error/1", "code": "pending", "message": "no run for abc yet", "hint": "rbit run watch --sha abc" })
        );
        let rows = vec![json!({ "id": 1, "sha": "a", "url": "x" })];
        assert_eq!(select(rows.clone(), &[]), rows);
        assert_eq!(select(rows, &["id".to_owned(), "missing".to_owned()]), vec![json!({ "id": 1 })]);
    }
}
