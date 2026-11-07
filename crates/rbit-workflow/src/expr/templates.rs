//! Expression templates with original evaluation and allocation behavior.
use super::contexts::Contexts;
use super::syntax::{Expr, Function};
use super::values::{to_text, truthy};
use super::{ExprError, MAX_RESULT_BYTES, MAX_TEMPLATE_BYTES, eval, fail};
use serde_json::Value;
use std::fmt;

/// A string with `${{ … }}` expressions in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    /// The expression and its source text (without `${{ }}`).
    Expr(Expr, String),
}

impl Template {
    pub fn parse(text: &str) -> Result<Self, ExprError> {
        if text.len() > MAX_TEMPLATE_BYTES {
            return fail(format!("an expression template must be at most {MAX_TEMPLATE_BYTES} bytes"));
        }
        let mut parts = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("${{") {
            if start > 0 {
                parts.push(Part::Text(rest[..start].to_owned()));
            }
            let inner = &rest[start + 3..];
            let end = closing_braces(inner).ok_or_else(|| ExprError("a `${{` is not closed with `}}`".into()))?;
            let source = inner[..end].to_owned();
            parts.push(Part::Expr(Expr::parse(&source)?, source));
            rest = &inner[end + 2..];
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_owned()));
        }
        Ok(Self { parts })
    }

    /// Whether the template has no expression.
    pub fn is_literal(&self) -> bool {
        self.parts.iter().all(|part| matches!(part, Part::Text(_)))
    }

    /// The single expression the template consists of, if it is exactly `${{ … }}`.
    pub fn as_single_expr(&self) -> Option<&Expr> {
        match self.parts.as_slice() {
            [Part::Expr(expr, _)] => Some(expr),
            _ => None,
        }
    }

    /// The template's expressions, in order.
    pub fn exprs(&self) -> impl Iterator<Item = &Expr> {
        self.parts.iter().filter_map(|part| match part {
            Part::Expr(expr, _) => Some(expr),
            Part::Text(_) => None,
        })
    }

    /// The contexts the template reads, lower-cased, without repeats.
    pub fn contexts(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for name in self.exprs().flat_map(Expr::contexts) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// The functions the template calls, without repeats.
    pub fn functions(&self) -> Vec<Function> {
        let mut functions = Vec::new();
        for function in self.exprs().flat_map(Expr::functions) {
            if !functions.contains(&function) {
                functions.push(function);
            }
        }
        functions
    }

    /// The template's value: the expression's own value when the template is exactly one
    /// `${{ … }}`, otherwise the interpolated string.
    pub fn evaluate(&self, contexts: &dyn Contexts) -> Result<Value, ExprError> {
        match self.as_single_expr() {
            Some(expr) => expr.evaluate(contexts),
            None => self.render(contexts).map(Value::String),
        }
    }

    /// The template with every expression evaluated and interpolated as text. One budget of
    /// [`MAX_STEPS`] covers all of them.
    pub fn render(&self, contexts: &dyn Contexts) -> Result<String, ExprError> {
        let mut evaluator = eval::Evaluator::new(contexts);
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Expr(expr, _) => out.push_str(&to_text(evaluator.eval(expr)?.as_ref())),
            }
            if out.len() > MAX_RESULT_BYTES {
                return fail(format!("the result is larger than {MAX_RESULT_BYTES} bytes"));
            }
        }
        Ok(out)
    }

    /// Evaluates the expressions that can be evaluated with `contexts` and leaves the others
    /// as written.
    pub fn render_partially(&self, contexts: &dyn Contexts) -> String {
        let mut evaluator = eval::Evaluator::new(contexts);
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Expr(expr, source) => match evaluator.eval(expr) {
                    Ok(value) if out.len() + to_text(&value).len() <= MAX_RESULT_BYTES => out.push_str(&to_text(&value)),
                    _ => {
                        out.push_str("${{");
                        out.push_str(source);
                        out.push_str("}}");
                    }
                },
            }
        }
        out
    }
}

/// The offset of the `}}` closing an expression, skipping quoted strings.
fn closing_braces(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' => quoted = !quoted,
            b'}' if !quoted && bytes.get(index + 1) == Some(&b'}') => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

/// An `if:` condition, ready to evaluate.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition(Expr);

impl Condition {
    /// `success()`: the condition of a job or step without `if`.
    pub fn success() -> Self {
        Self(Expr::Call(Function::Success, Vec::new()))
    }

    /// Reads a condition as GitHub does: written with or without `${{ }}`, empty meaning
    /// `success()`, and `success() && (…)` around one that calls no status function.
    /// Text around a `${{ }}` (`${{ a }} == b`) is refused: GitHub would read that as a
    /// non-empty string, which is always true.
    pub fn parse(text: &str) -> Result<Self, ExprError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::success());
        }
        let expr = if text.contains("${{") {
            Template::parse(text)?.as_single_expr().cloned().ok_or_else(|| {
                ExprError("a condition must be one expression: write it without `${{ }}`, or all of it inside one `${{ }}`".into())
            })?
        } else {
            Expr::parse(text)?
        };
        Ok(Self(if expr.uses_status() { expr } else { Expr::And(vec![Expr::Call(Function::Success, Vec::new()), expr]) }))
    }

    /// The expression evaluated, the implicit `success() &&` included.
    pub fn expr(&self) -> &Expr {
        &self.0
    }

    /// Whether the condition holds.
    pub fn evaluate(&self, contexts: &dyn Contexts) -> Result<bool, ExprError> {
        Ok(truthy(eval::Evaluator::new(contexts).eval(&self.0)?.as_ref()))
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
