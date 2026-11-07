//! Expression values with original evaluation and allocation behavior.
use super::lex;
use serde_json::Value;

/// Whether a value counts as true: everything but `false`, `0`, `-0`, `''` and `null`.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => {
            let number = to_number(value);
            number != 0.0 && !number.is_nan()
        }
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// A value as GitHub interpolates it into text.
pub fn to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(integer), _) => integer.to_string(),
            (_, Some(integer)) => integer.to_string(),
            _ => format_number(number.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(text) => text.clone(),
        Value::Array(_) => "Array".to_owned(),
        Value::Object(_) => "Object".to_owned(),
    }
}

/// A value as a number, for comparisons between kinds: `null` is 0, booleans 1 and 0,
/// strings parse as numbers (empty is 0, otherwise `NaN` when they are not one), arrays and
/// objects are `NaN`.
pub fn to_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(value) => f64::from(u8::from(*value)),
        Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
        Value::String(text) => parse_number_text(text),
        Value::Array(_) | Value::Object(_) => f64::NAN,
    }
}

fn parse_number_text(text: &str) -> f64 {
    let text = text.trim();
    if text.is_empty() {
        return 0.0;
    }
    let (negative, digits) = match text.as_bytes()[0] {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    #[allow(clippy::cast_precision_loss, reason = "expressions compute with doubles, as on GitHub")]
    let value = match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).map_or(f64::NAN, |value| value as f64),
        None if lex::is_decimal(digits) => digits.parse().unwrap_or(f64::NAN),
        None => f64::NAN,
    };
    if negative { -value } else { value }
}

/// A finite number as a [`Value`]: an integer when it is one (so `toJSON(2)` is `2`), a
/// float otherwise; `-0` stays a float so it prints as `-0`.
pub fn number_value(value: f64) -> Value {
    const EXACT: f64 = 9_007_199_254_740_992.0;
    if value.fract() == 0.0 && value.abs() < EXACT && !(value == 0.0 && value.is_sign_negative()) {
        #[allow(clippy::cast_possible_truncation, reason = "integral and within ±2^53")]
        return Value::from(value as i64);
    }
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// Numbers as GitHub prints them: integers without a fraction.
pub fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity".to_owned() } else { "-Infinity".to_owned() }
    } else if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}
