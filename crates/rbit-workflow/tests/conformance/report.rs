//! Conformance report with unchanged verdict and fixture semantics.
use super::cases::Case;
use serde_json::Value;
use std::any::Any;
use std::fmt::Write as _;

pub(super) fn describe(case: &Case, actual: &Value) -> String {
    let mut text = format!("{} [{}, {}] {} area={} tags={}", case.id, case.split, case.kind, case.file, case.area, case.tags.join(","));
    if let Some(why) = &case.why_hard {
        let _ = write!(text, "\n    why hard: {why}");
    }
    let _ = write!(
        text,
        "\n    input:    {}\n    expected: {}\n    actual:   {}\n    rule:     {}\n",
        case.input, case.expect, actual, case.source
    );
    text
}

/// The share of `passed` in `total`, as a percentage.
#[allow(clippy::cast_precision_loss, reason = "case counts are far below 2^52")]
pub(super) fn percent(passed: usize, total: usize) -> f64 {
    100.0 * passed as f64 / total as f64
}

pub(super) fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".into())
}
