//! The functions of §12.4: `contains`, `startsWith`, `endsWith`, `format`, `join`,
//! `toJSON`, `fromJSON`, the status functions, and `hashFiles` through the evaluating
//! side's hook.
//!
//! Text functions compare ignoring case, as GitHub's do. Functions whose cost grows with
//! their input or output also spend one step per KiB, so a few large calls exhaust the
//! budget as surely as many small ones.

use std::borrow::Cow;

use serde_json::Value;

use super::{
    Expr, ExprError, Function, MAX_JSON_INPUT_BYTES, MAX_RESULT_BYTES, Status,
    eval::{Evaluator, Val, loose_eq},
    fail, to_text,
};

pub(super) fn call<'c>(evaluator: &mut Evaluator<'c>, function: Function, arguments: &[Expr]) -> Result<Val<'c>, ExprError> {
    let value = match function {
        Function::Success | Function::Failure | Function::Always | Function::Cancelled => {
            let Some(status) = evaluator.contexts.status() else {
                return fail(format!("`{}()` can only be used in `if` conditions", function.name()));
            };
            Value::Bool(match function {
                Function::Success => status == Status::Success,
                Function::Failure => status == Status::Failure,
                Function::Cancelled => status == Status::Cancelled,
                _ => true,
            })
        }
        Function::Contains => {
            let search = evaluator.eval(&arguments[0])?;
            let item = evaluator.eval(&arguments[1])?;
            Value::Bool(match search.as_ref() {
                Value::Array(items) => {
                    evaluator.charge(items.len())?;
                    items.iter().any(|candidate| loose_eq(&Cow::Borrowed(candidate), &item))
                }
                _ => text_test(evaluator, &search, &item, |haystack, needle| haystack.contains(needle))?,
            })
        }
        Function::StartsWith | Function::EndsWith => {
            let text = evaluator.eval(&arguments[0])?;
            let affix = evaluator.eval(&arguments[1])?;
            let starts = function == Function::StartsWith;
            Value::Bool(text_test(
                evaluator,
                &text,
                &affix,
                |text, affix| if starts { text.starts_with(affix) } else { text.ends_with(affix) },
            )?)
        }
        Function::Format => Value::String(format(evaluator, arguments)?),
        Function::Join => {
            let value = evaluator.eval(&arguments[0])?;
            let separator = match arguments.get(1) {
                Some(separator) => to_text(evaluator.eval(separator)?.as_ref()),
                None => ",".to_owned(),
            };
            Value::String(match value.as_ref() {
                Value::Array(items) => {
                    evaluator.charge(items.len())?;
                    let mut out = String::new();
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            out.push_str(&separator);
                        }
                        out.push_str(&to_text(item));
                        check_size("join", &out)?;
                    }
                    evaluator.charge(out.len() / 1024)?;
                    out
                }
                Value::Object(_) => String::new(),
                other => to_text(other),
            })
        }
        Function::ToJson => {
            let value = evaluator.eval(&arguments[0])?;
            let json = serde_json::to_string_pretty(value.as_ref()).map_err(|error| ExprError(format!("toJSON: {error}")))?;
            check_size("toJSON", &json)?;
            evaluator.charge(json.len() / 1024)?;
            Value::String(json)
        }
        Function::FromJson => {
            let text = to_text(evaluator.eval(&arguments[0])?.as_ref());
            if text.len() > MAX_JSON_INPUT_BYTES {
                return fail(format!("fromJSON: the input is larger than {MAX_JSON_INPUT_BYTES} bytes"));
            }
            evaluator.charge(text.len() / 1024)?;
            serde_json::from_str(&text).map_err(|error| ExprError(format!("fromJSON: the input is not JSON ({error})")))?
        }
        Function::HashFiles => {
            let mut patterns = Vec::with_capacity(arguments.len());
            for argument in arguments {
                patterns.push(to_text(evaluator.eval(argument)?.as_ref()));
            }
            match evaluator.contexts.hash_files(&patterns) {
                None => return fail("`hashFiles` is only available on the runner"),
                Some(Ok(hash)) => Value::String(hash),
                Some(Err(problem)) => return fail(format!("hashFiles: {problem}")),
            }
        }
    };
    Ok(Cow::Owned(value))
}

/// Applies `test` to both values as upper-cased text.
fn text_test(evaluator: &mut Evaluator<'_>, left: &Value, right: &Value, test: impl Fn(&str, &str) -> bool) -> Result<bool, ExprError> {
    let fold = |value: &Value| to_text(value).chars().flat_map(char::to_uppercase).collect::<String>();
    let (left, right) = (fold(left), fold(right));
    evaluator.charge((left.len() + right.len()) / 1024)?;
    Ok(test(&left, &right))
}

/// `format(template, values…)`: `{N}` is the Nth value, `{{` and `}}` are braces.
fn format(evaluator: &mut Evaluator<'_>, arguments: &[Expr]) -> Result<String, ExprError> {
    let template = to_text(evaluator.eval(&arguments[0])?.as_ref());
    let mut values = Vec::with_capacity(arguments.len() - 1);
    for argument in &arguments[1..] {
        values.push(to_text(evaluator.eval(argument)?.as_ref()));
    }
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.next_if_eq(&'{').is_some() => out.push('{'),
            '{' => {
                let mut index = String::new();
                while let Some(digit) = chars.next_if(char::is_ascii_digit) {
                    index.push(digit);
                }
                if index.is_empty() || chars.next_if_eq(&'}').is_none() {
                    return fail("format: a `{` must start an argument like `{0}`; write `{{` for a brace");
                }
                let value = index
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| values.get(index))
                    .ok_or_else(|| ExprError(format!("format: `{{{index}}}` has no value; {} were given", values.len())))?;
                out.push_str(value);
            }
            '}' if chars.next_if_eq(&'}').is_some() => out.push('}'),
            '}' => return fail("format: a single `}` must be written `}}`"),
            c => out.push(c),
        }
        check_size("format", &out)?;
    }
    evaluator.charge(out.len() / 1024)?;
    Ok(out)
}

fn check_size(function: &str, text: &str) -> Result<(), ExprError> {
    if text.len() > MAX_RESULT_BYTES { fail(format!("{function}: the result is larger than {MAX_RESULT_BYTES} bytes")) } else { Ok(()) }
}
