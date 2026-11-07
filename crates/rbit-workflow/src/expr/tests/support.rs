//! Existing expression support acceptance cases.
use super::super::{ContextMap, Expr, ExprError, Status, Template};
use serde_json::{Value, json};

/// The contexts of the subset rbit evaluated before this evaluator, plus the data of
/// GitHub's documented examples.
pub(super) fn contexts() -> ContextMap {
    ContextMap::new()
        .with(
            "github",
            json!({
                "ref": "refs/heads/main",
                "head_ref": "",
                "run_id": 42,
                "event_name": "push",
                "event": {
                    "pull_request": {"number": null},
                    "issue": {"labels": [{"name": "bug"}, {"name": "help wanted"}]},
                },
            }),
        )
        .with("matrix", json!({"os": "linux", "zero": 0, "experimental": true}))
        .with("inputs", json!({}))
        .with("needs", json!({"build": {"result": "success", "outputs": {"version": "1.2"}}, "lint": {"result": "skipped"}}))
        .with(
            "vars",
            json!({
                "fruits": [{"name": "apple", "quantity": 1}, {"name": "orange", "quantity": 2}, {"name": "pear", "quantity": 1}],
                "vegetables": {
                    "scallions": {"colors": ["green", "white", "red"], "ediblePortions": ["roots", "stalks"]},
                    "beets": {"colors": ["purple", "red", "gold", "white", "pink"], "ediblePortions": ["roots", "stems", "leaves"]},
                    "artichokes": {"colors": ["green", "purple", "red", "black"], "ediblePortions": ["hearts", "stems", "leaves"]},
                },
            }),
        )
}

pub(super) fn render(template: &str) -> Result<String, ExprError> {
    Template::parse(template)?.render(&contexts())
}

pub(super) fn value(expression: &str) -> Value {
    Expr::parse(expression)
        .and_then(|expr| expr.evaluate(&contexts().with_status(Status::Success)))
        .unwrap_or_else(|error| panic!("{expression}: {error}"))
}
