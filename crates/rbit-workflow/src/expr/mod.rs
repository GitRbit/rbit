//! GitHub's expression language, evaluated the same way by the server and by `rbit-runner`
//! (§12.4 of `docs/design/actions.md`). A template therefore means the same thing on both
//! sides; what differs is only which contexts each side can supply.
//!
//! # The language
//!
//! * Literals: `null`, `true`, `false`, numbers (`711`, `-9.2`, `-2.99e-2`, `0xff`) and
//!   `'strings'`, with `''` for a quote.
//! * Contexts and paths: `github.event.pull_request.number`, `matrix['os']`,
//!   `fromJSON(x)[0]`, and the object filter `.*` (`needs.*.result` is the list of every
//!   need's result). Context and property names are case-insensitive; a property that does
//!   not exist is `null`, but a context the evaluating side does not supply is an error.
//! * Operators, tightest first: `!`; `<` `<=` `>` `>=`; `==` `!=`; `&&`; `||`; and
//!   parentheses. Comparisons coerce as GitHub does: values of different kinds compare as
//!   numbers (`null` is 0, booleans 1 and 0, strings parse as JSON numbers, otherwise
//!   `NaN`, which equals nothing), strings compare ignoring case, and arrays and objects are
//!   equal only when they are the same instance. `a && b` and `a || b` return an operand,
//!   not a boolean. Falsy are `false`, `0`, `-0`, `''` and `null`.
//! * Functions: `contains`, `startsWith`, `endsWith`, `format` (`{0}`, with `{{` and `}}`
//!   for braces), `join`, `toJSON`, `fromJSON`, the status functions `success`, `failure`,
//!   `always` and `cancelled`, and `hashFiles`, which only the runner supplies.
//! * Values are [`Value`] (`serde_json`). Interpolated into text, `null` is empty, numbers
//!   print without a needless fraction, arrays print as `Array` and objects as `Object`.
//!
//! # Limits
//!
//! A template or expression is at most [`MAX_TEMPLATE_BYTES`], nested at most
//! [`MAX_NESTING`] levels, evaluated in at most [`MAX_STEPS`] steps; `fromJSON` reads at
//! most [`MAX_JSON_INPUT_BYTES`] and no result may exceed [`MAX_RESULT_BYTES`]. Exceeding
//! any of them is an [`ExprError`], never a panic.
//!
//! # API
//!
//! * [`Expr::parse`] parses one expression written without `${{ }}`; [`Expr`] prints back
//!   as canonical text (`Display`) and lists the [`Expr::contexts`] and
//!   [`Expr::functions`] it uses, so a caller can refuse, say, `secrets` in `runs-on` when
//!   a workflow is parsed rather than when it runs.
//! * [`Template::parse`] parses a string with `${{ … }}` in it. [`Template::evaluate`]
//!   gives a [`Value`] — the expression's own value when the template is exactly one
//!   `${{ … }}` (so `continue-on-error: ${{ matrix.experimental }}` is a boolean), the
//!   interpolated string otherwise — and [`Template::render`] always gives the string.
//! * [`Condition::parse`] reads an `if:`: with or without `${{ }}`, empty meaning
//!   `success()`, and, as on GitHub, a condition that calls no status function means
//!   `success() && (condition)`. [`Condition::evaluate`] answers it.
//! * Each side implements [`Contexts`] for what it knows: the contexts by name, the job
//!   [`Status`] for the status functions, and optionally the `hashFiles` hook. The server
//!   supplies `github`, `inputs`, `vars`, `matrix`, `strategy` and, when a job unblocks,
//!   `needs`; the runner adds `env`, `secrets`, `steps`, `job` and `runner`.
//!   [`ContextMap`] is a ready-made implementation over owned values.
//!
//! ```
//! use rbit_workflow::expr::{Condition, ContextMap, Status, Template};
//! use serde_json::json;
//!
//! let contexts = ContextMap::new()
//!     .with("github", json!({"ref": "refs/heads/main", "event_name": "push"}))
//!     .with("matrix", json!({"os": "linux", "experimental": true}));
//! let name = Template::parse("Build on ${{ matrix.os }}")?.render(&contexts)?;
//! assert_eq!(name, "Build on linux");
//! let flag = Template::parse("${{ matrix.experimental }}")?.evaluate(&contexts)?;
//! assert_eq!(flag, json!(true));
//!
//! let condition = Condition::parse("github.ref == 'refs/heads/main'")?;
//! assert_eq!(condition.to_string(), "success() && github.ref == 'refs/heads/main'");
//! assert!(condition.evaluate(&contexts.with_status(Status::Success))?);
//! # Ok::<(), rbit_workflow::expr::ExprError>(())
//! ```

mod eval;
mod functions;
mod lex;
mod parse;

use std::{collections::BTreeMap, fmt};

pub use parse::{Accessor, Comparison, Expr, Function};
pub use serde_json::Value;

use crate::spec::JobResult;

/// Longest template or expression, in bytes.
pub const MAX_TEMPLATE_BYTES: usize = 4096;
/// Deepest nesting of parentheses, brackets, calls, `!` and chained comparisons.
pub const MAX_NESTING: usize = 32;
/// Most evaluation steps one template or condition may take.
pub const MAX_STEPS: usize = 10_000;
/// Largest input of `fromJSON`, in bytes.
pub const MAX_JSON_INPUT_BYTES: usize = 1024 * 1024;
/// Largest string an evaluation may produce, in bytes.
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;

/// Every context name an expression may use. Which of them a side supplies is up to its
/// [`Contexts`].
// Actions: reusable workflows. `jobs` is only for a called workflow's `outputs`.
pub const CONTEXTS: [&str; 12] =
    ["github", "env", "vars", "job", "steps", "runner", "secrets", "strategy", "matrix", "needs", "inputs", "jobs"];

/// Why an expression cannot be parsed or evaluated, for the workflow's author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

fn fail<T>(message: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError(message.into()))
}

/// What the status functions see.
///
/// For a job's `if` this is the outcome of its `needs`; for a step's `if`, the outcome of
/// the steps before it ([`Status::Skipped`] does not occur there).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// Everything before succeeded: `success()` is true.
    Success,
    /// Something before failed: `failure()` is true.
    Failure,
    /// The run was cancelled: `cancelled()` is true.
    Cancelled,
    /// A needed job was skipped: `success()`, `failure()` and `cancelled()` are all false,
    /// so the job is skipped too unless its condition says `always()`.
    Skipped,
}

impl Status {
    /// The status a job's `if` sees from the results of its needs: a failure wins, then a
    /// cancellation, then a skip; no needs is success.
    pub fn of_needs(results: impl IntoIterator<Item = JobResult>) -> Self {
        let mut status = Self::Success;
        for result in results {
            status = match (status, result) {
                (_, JobResult::Failure) | (Self::Failure, _) => Self::Failure,
                (_, JobResult::Cancelled) | (Self::Cancelled, _) => Self::Cancelled,
                (_, JobResult::Skipped) | (Self::Skipped, _) => Self::Skipped,
                (Self::Success, JobResult::Success) => Self::Success,
            };
        }
        status
    }
}

/// What one side supplies to an evaluation.
pub trait Contexts {
    /// The context called `name` (lower-case, one of [`CONTEXTS`]), or `None` when this
    /// side does not supply it here. Reading an unsupplied context is an evaluation error,
    /// never `null`, so the server cannot quietly evaluate what only the runner knows.
    fn context(&self, name: &str) -> Option<&Value>;

    /// The status the status functions read, or `None` where they cannot be used (outside
    /// `if` conditions).
    fn status(&self) -> Option<Status> {
        None
    }

    /// `hashFiles(patterns…)`: the SHA-256 the runner computes over the matching files of
    /// the workspace, `Some(Err(reason))` when it cannot, or `None` where `hashFiles` is not
    /// available (on the server).
    fn hash_files(&self, patterns: &[String]) -> Option<Result<String, String>> {
        let _ = patterns;
        None
    }
}

/// [`Contexts`] over owned values: each context by name, and optionally a [`Status`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextMap {
    contexts: BTreeMap<String, Value>,
    status: Option<Status>,
}

impl ContextMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// With the context `name` (case-insensitive) set to `value`.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.insert(name, value);
        self
    }

    /// With the status the status functions read.
    #[must_use]
    pub fn with_status(&self, status: Status) -> Self {
        Self { status: Some(status), ..self.clone() }
    }

    pub fn insert(&mut self, name: &str, value: impl Into<Value>) {
        self.contexts.insert(name.to_ascii_lowercase(), value.into());
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.contexts.remove(&name.to_ascii_lowercase())
    }

    pub fn set_status(&mut self, status: Option<Status>) {
        self.status = status;
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.contexts.get(&name.to_ascii_lowercase())
    }
}

impl Contexts for ContextMap {
    fn context(&self, name: &str) -> Option<&Value> {
        self.contexts.get(name)
    }

    fn status(&self) -> Option<Status> {
        self.status
    }
}

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

impl Expr {
    /// Evaluates against `contexts`.
    pub fn evaluate(&self, contexts: &dyn Contexts) -> Result<Value, ExprError> {
        let value = eval::Evaluator::new(contexts).eval(self)?.into_owned();
        check_result(&value)?;
        Ok(value)
    }
}

fn check_result(value: &Value) -> Result<(), ExprError> {
    match value {
        Value::String(text) if text.len() > MAX_RESULT_BYTES => fail(format!("the result is larger than {MAX_RESULT_BYTES} bytes")),
        _ => Ok(()),
    }
}

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

/// Parses and evaluates `template` in one go (see [`Template::evaluate`]).
pub fn evaluate(template: &str, contexts: &dyn Contexts) -> Result<Value, ExprError> {
    Template::parse(template)?.evaluate(contexts)
}

/// Parses and renders `template` in one go (see [`Template::render`]).
pub fn render(template: &str, contexts: &dyn Contexts) -> Result<String, ExprError> {
    Template::parse(template)?.render(contexts)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The contexts of the subset rbit evaluated before this evaluator, plus the data of
    /// GitHub's documented examples.
    fn contexts() -> ContextMap {
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

    fn render(template: &str) -> Result<String, ExprError> {
        Template::parse(template)?.render(&contexts())
    }

    fn value(expression: &str) -> Value {
        Expr::parse(expression)
            .and_then(|expr| expr.evaluate(&contexts().with_status(Status::Success)))
            .unwrap_or_else(|error| panic!("{expression}: {error}"))
    }

    /// The templates rbit evaluated before this evaluator existed: a strict sub-language,
    /// so every one of them still means the same.
    #[test]
    fn the_former_subset() {
        let cases = [
            ("plain", "plain"),
            ("", ""),
            ("${{ github.ref }}", "refs/heads/main"),
            ("ci-${{ github.ref }}-x", "ci-refs/heads/main-x"),
            ("${{github.REF}}", "refs/heads/main"),
            ("${{ github.head_ref || github.ref }}", "refs/heads/main"),
            ("${{ github.head_ref || github.run_id }}", "42"),
            ("${{ github.event.pull_request.number || github.ref }}", "refs/heads/main"),
            ("${{ github.ref && 'yes' }}", "yes"),
            ("${{ github.head_ref && 'yes' }}", ""),
            ("${{ matrix.zero || 'zero is falsy' }}", "zero is falsy"),
            ("${{ null || false }}", "false"),
            ("${{ 'it''s' }}", "it's"),
            ("${{ '}}' }}", "}}"),
            ("${{ (null || 'a') && ('b' || 'c') }}", "b"),
            ("${{ 1.5 }} ${{ 2 }} ${{ -0 }} ${{ 0x10 }}", "1.5 2 -0 16"),
            ("${{ true }}${{ matrix.os }}", "truelinux"),
            ("a ${{ 'b' }} c ${{ 'd' }}", "a b c d"),
            // Formerly refused, now evaluated.
            ("${{ format('{0}', github.ref) }}", "refs/heads/main"),
            ("${{ github.ref == 'x' }}", "false"),
            ("${{ !github.ref }}", "false"),
            ("${{ github['ref'] }}", "refs/heads/main"),
            ("${{ github.sha }}", ""),
        ];
        for (template, expected) in cases {
            assert_eq!(render(template).as_deref(), Ok(expected), "{template}");
        }
    }

    #[test]
    fn refusals() {
        for template in [
            "${{ secrets.TOKEN }}",
            "${{ github.ref | x }}",
            "${{ github. }}",
            "${{ 'open }}",
            "${{ github.ref",
            "${{ (github.ref }}",
            "${{ }}",
            "${{ 1x }}",
            "${{ success() }}",
            "${{ hashFiles('**/Cargo.lock') }}",
            "${{ x.y }}",
        ] {
            assert!(render(template).is_err(), "{template}");
        }
        let deep = format!("${{{{ {}'x'{} }}}}", "(".repeat(40), ")".repeat(40));
        assert!(render(&deep).is_err());
        assert!(Template::parse(&"x".repeat(MAX_TEMPLATE_BYTES + 1)).is_err());
        assert_eq!(render("${{ secrets.TOKEN }}").unwrap_err().0, "the `secrets` context is not available here");
    }

    #[test]
    fn partial_rendering_keeps_what_cannot_be_evaluated() {
        assert!(Template::parse("Build ${{ matrix.os }} (${{ format( }})").is_err(), "bad syntax is still refused when parsing");
        let template = Template::parse("Build ${{ matrix.os }} for ${{ secrets.target }}").unwrap();
        assert_eq!(template.render_partially(&contexts()), "Build linux for ${{ secrets.target }}");
        assert!(!template.is_literal());
        assert_eq!(template.contexts(), ["matrix", "secrets"]);
        assert!(Template::parse("literal").unwrap().is_literal());
        let single = Template::parse("${{ matrix.os }}").unwrap();
        assert_eq!(single.as_single_expr().map(Expr::contexts), Some(vec!["matrix"]));
        assert!(Template::parse("x ${{ matrix.os }}").unwrap().as_single_expr().is_none());
    }

    #[test]
    fn single_expressions_keep_their_type() {
        let evaluate = |template: &str| Template::parse(template).unwrap().evaluate(&contexts()).unwrap();
        assert_eq!(evaluate("${{ matrix.experimental }}"), json!(true));
        assert_eq!(evaluate("${{ github.run_id }}"), json!(42));
        assert_eq!(evaluate("${{ fromJSON('[1, 2]') }}"), json!([1, 2]));
        assert_eq!(evaluate(" ${{ matrix.experimental }}"), json!(" true"));
        assert_eq!(evaluate("plain"), json!("plain"));
        assert_eq!(super::evaluate("${{ 3 }}", &contexts()).unwrap(), json!(3));
        assert_eq!(super::render("${{ 3 }}", &contexts()).unwrap(), "3");
    }

    /// GitHub's documented literals, operators and coercions.
    #[test]
    fn literals_operators_and_coercion() {
        let cases = [
            ("null", json!(null)),
            ("false", json!(false)),
            ("711", json!(711)),
            ("-9.2", json!(-9.2)),
            ("0xff", json!(255)),
            ("-2.99e-2", json!(-0.0299)),
            ("'Mona the Octocat'", json!("Mona the Octocat")),
            ("'It''s open source!'", json!("It's open source!")),
            // Equality coerces different kinds to numbers.
            ("null == 0", json!(true)),
            ("'' == 0", json!(true)),
            ("'' == null", json!(true)),
            ("true == 1", json!(true)),
            ("false == '0'", json!(true)),
            ("1 == '1.0'", json!(true)),
            ("1 == ' 1 '", json!(true)),
            ("'0x10' == 16", json!(true)),
            ("'abc' == 1", json!(false)),
            ("'abc' != 1", json!(true)),
            ("'Abc' == 'aBC'", json!(true)),
            ("'straße' == 'STRASSE'", json!(true)),
            ("1 == 1.0", json!(true)),
            ("fromJSON('[]') == fromJSON('[]')", json!(false)),
            ("fromJSON('[]') == 0", json!(false)),
            ("github.event == github.event", json!(true)),
            ("github.event == github['EVENT']", json!(true)),
            ("github.event.issue == github.event", json!(false)),
            ("vars.fruits == vars.fruits", json!(true)),
            // Ordering.
            ("1 < 2", json!(true)),
            ("'a' < 'B'", json!(true)),
            ("'b' <= 'B'", json!(true)),
            ("null < 1", json!(true)),
            ("1 < '2'", json!(true)),
            ("'abc' < 1", json!(false)),
            ("'abc' >= 1", json!(false)),
            ("2 >= 2", json!(true)),
            ("true > false", json!(true)),
            ("vars.fruits > 0", json!(false)),
            // Logic returns operands.
            ("'a' && 'b'", json!("b")),
            ("0 && 'x'", json!(0)),
            ("'' || 'x'", json!("x")),
            ("null || 0 || ''", json!("")),
            ("!null", json!(true)),
            ("!vars.fruits", json!(false)),
            ("!!'x'", json!(true)),
            ("1 < 2 == true", json!(true)),
            ("!(1 == 2) && 3 > 2", json!(true)),
            // Paths.
            ("github.event.issue.labels[1].name", json!("help wanted")),
            ("github.event.issue.labels[5]", json!(null)),
            ("github.event.issue.labels[-1]", json!(null)),
            ("github.event.issue.labels[0.9].name", json!("bug")),
            ("github.event.issue.labels['0']", json!(null)),
            ("github.missing.deeper", json!(null)),
            ("github.ref.length", json!(null)),
            ("needs.build.outputs.version", json!("1.2")),
            ("needs['build']['OUTPUTS'].version", json!("1.2")),
            ("vars.fruits.*.name", json!(["apple", "orange", "pear"])),
            ("vars.fruits.*.missing", json!([])),
            ("needs.*.result", json!(["success", "skipped"])),
            ("vars.vegetables.*.ediblePortions", json!([["hearts", "stems", "leaves"], ["roots", "stems", "leaves"], ["roots", "stalks"]])),
            ("vars.vegetables.*.ediblePortions.*", json!(["hearts", "stems", "leaves", "roots", "stems", "leaves", "roots", "stalks"])),
            ("vars.fruits.*[1]", json!([])),
            ("github.ref.*", json!([])),
            ("fromJSON('{\"a\": [1, 2]}').a[1]", json!(2)),
            ("fromJSON('[{\"a\": 1}, {\"b\": 2}]').*.a", json!([1])),
        ];
        for (expression, expected) in cases {
            assert_eq!(value(expression), expected, "{expression}");
        }
    }

    /// GitHub's documented function examples.
    #[test]
    fn functions() {
        let cases = [
            ("contains('Hello world', 'llo')", json!(true)),
            ("contains('Hello world', 'LLO')", json!(true)),
            ("contains('Hello world', 'xyz')", json!(false)),
            ("contains(github.event.issue.labels.*.name, 'bug')", json!(true)),
            ("contains(github.event.issue.labels.*.name, 'BUG')", json!(true)),
            ("contains(fromJSON('[\"push\", \"pull_request\"]'), github.event_name)", json!(true)),
            ("contains(fromJSON('[1, 2]'), '2')", json!(true)),
            ("contains(123, 2)", json!(true)),
            ("startsWith('Hello world', 'He')", json!(true)),
            ("startsWith('Hello world', 'he')", json!(true)),
            ("startsWith('Hello world', 'world')", json!(false)),
            ("endsWith('Hello world', 'ld')", json!(true)),
            ("endsWith('Hello world', 'LD')", json!(true)),
            ("format('Hello {0} {1} {2}', 'Mona', 'the', 'Octocat')", json!("Hello Mona the Octocat")),
            ("format('{{Hello {0} {1} {2}!}}', 'Mona', 'the', 'Octocat')", json!("{Hello Mona the Octocat!}")),
            ("format('{0}{0}{1}', 1, null)", json!("11")),
            ("format('{0}', vars.fruits)", json!("Array")),
            ("format('{0}', github.event)", json!("Object")),
            ("join(github.event.issue.labels.*.name, ', ')", json!("bug, help wanted")),
            ("join(fromJSON('[1, true, null, \"x\"]'))", json!("1,true,,x")),
            ("join('abc', '-')", json!("abc")),
            ("join(github.event)", json!("")),
            ("join(fromJSON('[]'))", json!("")),
            ("toJSON(fromJSON('{\"a\": [1]}'))", json!("{\n  \"a\": [\n    1\n  ]\n}")),
            ("toJSON('x')", json!("\"x\"")),
            ("toJSON(2)", json!("2")),
            ("toJSON(null)", json!("null")),
            ("fromJSON('{\"a\": 1}').a", json!(1)),
            ("fromJSON('true')", json!(true)),
            ("fromJSON(' 3.5 ')", json!(3.5)),
            ("fromJSON(toJSON(vars.fruits)) == vars.fruits", json!(false)),
            ("success()", json!(true)),
            ("failure()", json!(false)),
            ("cancelled()", json!(false)),
            ("always()", json!(true)),
        ];
        for (expression, expected) in cases {
            assert_eq!(value(expression), expected, "{expression}");
        }
        for (expression, message) in [
            ("format('{0} {1}', 'a')", "`{1}` has no value"),
            ("format('{x}', 'a')", "must start an argument"),
            ("format('{0', 'a')", "must start an argument"),
            ("format('}', 'a')", "single `}`"),
            ("fromJSON('{')", "not JSON"),
            ("fromJSON('')", "not JSON"),
            ("hashFiles('*.lock')", "only available on the runner"),
        ] {
            let error = Expr::parse(expression).unwrap().evaluate(&contexts().with_status(Status::Success)).unwrap_err();
            assert!(error.0.contains(message), "{expression}: {error}");
        }
    }

    #[test]
    fn status_functions_follow_the_status() {
        let holds =
            |condition: &str, status: Status| Condition::parse(condition).unwrap().evaluate(&contexts().with_status(status)).unwrap();
        let table = [
            ("success()", [true, false, false, false]),
            ("failure()", [false, true, false, false]),
            ("cancelled()", [false, false, true, false]),
            ("always()", [true, true, true, true]),
            ("", [true, false, false, false]),
            ("true", [true, false, false, false]),
            ("!cancelled()", [true, true, false, true]),
            ("failure() || cancelled()", [false, true, true, false]),
        ];
        for (condition, expected) in table {
            let got = [Status::Success, Status::Failure, Status::Cancelled, Status::Skipped].map(|status| holds(condition, status));
            assert_eq!(got, expected, "{condition}");
        }
        assert!(Expr::parse("success()").unwrap().evaluate(&contexts()).unwrap_err().0.contains("only be used in `if`"));
        let of_needs = |results: &[JobResult]| Status::of_needs(results.iter().copied());
        assert_eq!(of_needs(&[]), Status::Success);
        assert_eq!(of_needs(&[JobResult::Success, JobResult::Success]), Status::Success);
        assert_eq!(of_needs(&[JobResult::Skipped, JobResult::Success]), Status::Skipped);
        assert_eq!(of_needs(&[JobResult::Skipped, JobResult::Cancelled]), Status::Cancelled);
        assert_eq!(of_needs(&[JobResult::Failure, JobResult::Cancelled, JobResult::Skipped]), Status::Failure);
        assert_eq!(of_needs(&[JobResult::Cancelled, JobResult::Failure]), Status::Failure);
    }

    #[test]
    fn conditions() {
        let cases = [
            ("", "success()"),
            ("  ", "success()"),
            ("true", "success() && true"),
            ("github.ref == 'refs/heads/main'", "success() && github.ref == 'refs/heads/main'"),
            ("${{ github.ref == 'refs/heads/main' }}", "success() && github.ref == 'refs/heads/main'"),
            (" ${{ github.ref }} ", "success() && github.ref"),
            ("env.a || env.b", "success() && (env.a || env.b)"),
            ("always() && env.x", "always() && env.x"),
            ("${{ !cancelled() }}", "!cancelled()"),
            ("contains(needs.*.result, 'failure') || failure()", "contains(needs.*.result, 'failure') || failure()"),
        ];
        for (text, expected) in cases {
            assert_eq!(Condition::parse(text).unwrap().to_string(), expected, "{text}");
        }
        assert_eq!(Condition::parse("").unwrap(), Condition::success());
        for refused in ["${{ github.ref }} == 'x'", "a ${{ github.ref }}", "${{ github.ref", "github.ref ==", "${{ }}"] {
            assert!(Condition::parse(refused).is_err(), "{refused}");
        }
        let skipped = contexts().with_status(Status::Skipped);
        assert!(!Condition::parse("true").unwrap().evaluate(&skipped).unwrap());
        assert!(Condition::parse("always()").unwrap().evaluate(&skipped).unwrap());
        assert!(Condition::parse("needs.lint.result == 'skipped'").unwrap().expr().uses_status());
    }

    #[test]
    fn hash_files_goes_through_the_hook() {
        struct Runner(ContextMap);
        impl Contexts for Runner {
            fn context(&self, name: &str) -> Option<&Value> {
                self.0.context(name)
            }
            fn hash_files(&self, patterns: &[String]) -> Option<Result<String, String>> {
                Some(if patterns.is_empty() { Err("no pattern".into()) } else { Ok(format!("hash of {}", patterns.join(" "))) })
            }
        }
        let runner = Runner(contexts());
        let key = Template::parse("cargo-${{ hashFiles('**/Cargo.lock', matrix.os) }}").unwrap().render(&runner).unwrap();
        assert_eq!(key, "cargo-hash of **/Cargo.lock linux");
    }

    #[test]
    fn limits_are_errors() {
        let big = "x".repeat(MAX_JSON_INPUT_BYTES + 1);
        let half = "y".repeat(MAX_RESULT_BYTES / 2 + 1);
        let numbers = json!((0..MAX_STEPS).collect::<Vec<_>>());
        let grid = json!(vec![vec![1; 200]; 60]);
        let contexts = ContextMap::new().with("env", json!({"big": big, "half": half, "numbers": numbers, "grid": grid}));
        let evaluate = |text: &str| Template::parse(text).and_then(|template| template.render(&contexts));
        let cases = [
            ("${{ fromJSON(env.big) }}", "fromJSON: the input is larger"),
            ("${{ format('{0}{0}', env.half) }}", "format: the result is larger"),
            ("${{ join(fromJSON('[1, 2, 3]'), env.half) }}", "join: the result is larger"),
            ("${{ env.half }}${{ env.half }}", "the result is larger"),
            ("${{ contains(env.numbers, -1) }}", "evaluation steps"),
            ("${{ join(env.numbers) }}", "evaluation steps"),
            ("${{ env.grid.*.* }}", "evaluation steps"),
            ("${{ contains(env.grid.*.*, 2) }}", "evaluation steps"),
        ];
        for (template, message) in cases {
            let error = evaluate(template).unwrap_err();
            assert!(error.0.contains(message), "{template}: {error}");
        }
        // A few large calls run out of steps, not only many small ones.
        let costly = format!("${{{{ {} }}}}", vec!["toJSON(env.grid)"; 200].join(" && "));
        assert!(evaluate(&costly).unwrap_err().0.contains("evaluation steps"));
        assert!(evaluate("${{ contains(env.grid.*[0], 1) }}").is_ok());
    }

    #[test]
    fn truthiness_and_text() {
        let falsy = [json!(null), json!(false), json!(0), json!(-0.0), json!("")];
        assert!(falsy.iter().all(|value| !truthy(value)));
        let truthy_values = [json!(true), json!(-1), json!("0"), json!("false"), json!([]), json!({})];
        assert!(truthy_values.iter().all(truthy));
        assert_eq!(format_number(1e20), "100000000000000000000");
        assert_eq!(format_number(0.1), "0.1");
        assert_eq!(format_number(f64::INFINITY), "Infinity");
        assert_eq!(format_number(f64::NAN), "NaN");
        assert_eq!(to_text(&json!(u64::MAX)), u64::MAX.to_string());
        assert_eq!(to_text(&number_value(2.0)), "2");
        assert_eq!(to_text(&number_value(-0.0)), "-0");
        assert_eq!(number_value(1e300), json!(1e300));
        for (text, number) in [("", 0.0), (" 12 ", 12.0), ("-1.5e1", -15.0), ("+3", 3.0), ("0x1f", 31.0), ("-0x1", -1.0)] {
            assert_eq!(to_number(&json!(text)), number, "{text}");
        }
        for text in ["abc", "inf", "NaN", "1x", "0x", "--1"] {
            assert!(to_number(&json!(text)).is_nan(), "{text}");
        }
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        fn leaf() -> impl Strategy<Value = Expr> {
            prop_oneof![
                Just(Expr::Null),
                any::<bool>().prop_map(Expr::Bool),
                (-1e6..1e6f64).prop_map(Expr::Number),
                any::<i32>().prop_map(|value| Expr::Number(f64::from(value))),
                ".{0,8}".prop_map(Expr::String),
                prop::sample::select(&CONTEXTS[..]).prop_map(|name| Expr::Context(name.to_owned())),
            ]
        }

        fn expr() -> impl Strategy<Value = Expr> {
            leaf().prop_recursive(5, 64, 4, |inner| {
                let accessor = prop_oneof![
                    "[a-zA-Z_][a-zA-Z0-9_-]{0,6}".prop_map(Accessor::Property),
                    inner.clone().prop_map(Accessor::Index),
                    Just(Accessor::Filter),
                ];
                let comparison = prop::sample::select(vec![
                    Comparison::Equal,
                    Comparison::NotEqual,
                    Comparison::Less,
                    Comparison::LessEqual,
                    Comparison::Greater,
                    Comparison::GreaterEqual,
                ]);
                let call = (prop::sample::select(&Function::ALL[..]), prop::collection::vec(inner.clone(), 0..4)).prop_map(
                    |(function, mut arguments)| {
                        let (least, most) = function.arity();
                        while arguments.len() < least {
                            arguments.push(Expr::String("x".into()));
                        }
                        arguments.truncate(most);
                        Expr::Call(function, arguments)
                    },
                );
                prop_oneof![
                    (inner.clone(), prop::collection::vec(accessor, 1..4))
                        .prop_map(|(base, accessors)| Expr::Access(Box::new(base), accessors)),
                    inner.clone().prop_map(|inner| Expr::Not(Box::new(inner))),
                    (comparison, inner.clone(), inner.clone()).prop_map(|(op, left, right)| Expr::Compare(
                        op,
                        Box::new(left),
                        Box::new(right)
                    )),
                    prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::And),
                    prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::Or),
                    call,
                ]
            })
        }

        fn expression_text() -> impl Strategy<Value = String> {
            let piece = prop_oneof![
                prop::sample::select(&CONTEXTS[..]).prop_map(str::to_owned),
                prop::sample::select(&Function::ALL[..]).prop_map(|function| function.name().to_owned()),
                prop::sample::select(vec![
                    ".", ".*", "[", "]", "(", ")", ",", "!", "<", "<=", ">", ">=", "==", "!=", "&&", "||", "'", "''", "null", "true", "-",
                    "0x", "1e", " ", "${{", "}}", "{0}", "a", "_b-1",
                ])
                .prop_map(str::to_owned),
                "[0-9]{1,3}(\\.[0-9]{1,2})?",
                "'[^']{0,4}'",
                any::<char>().prop_map(String::from),
            ];
            prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat())
        }

        /// Every context set, with data deep and wide enough for filters to cost.
        static RICH_CONTEXTS: std::sync::LazyLock<ContextMap> = std::sync::LazyLock::new(|| {
            let wide = json!(vec![json!({"a": [1, 2, 3], "b": "x", "c": {"d": [true, null]}}); 300]);
            let mut contexts = ContextMap::new();
            for name in CONTEXTS {
                contexts.insert(name, json!({"a": wide.clone(), "b": [wide.clone(), "text"], "n": 1.5, "s": "S"}));
            }
            contexts.with_status(Status::Failure)
        });

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn parsing_never_panics(text in expression_text()) {
                let _ = Expr::parse(&text);
                let _ = Template::parse(&text);
                let _ = Condition::parse(&text);
            }

            #[test]
            fn parsing_arbitrary_strings_never_panics(text in ".{0,64}") {
                let _ = Expr::parse(&text);
                let _ = Template::parse(&text);
            }

            #[test]
            fn printing_round_trips_trees(expr in expr()) {
                let printed = expr.to_string();
                prop_assume!(printed.len() <= MAX_TEMPLATE_BYTES);
                let parsed = Expr::parse(&printed).map_err(|error| TestCaseError::fail(format!("{printed}: {error}")))?;
                prop_assert_eq!(&parsed, &expr, "{}", printed);
            }

            #[test]
            fn printing_round_trips_text(text in expression_text()) {
                if let Ok(parsed) = Expr::parse(&text) {
                    let printed = parsed.to_string();
                    prop_assert_eq!(Expr::parse(&printed).ok(), Some(parsed), "{} printed as {}", text, printed);
                }
            }

            #[test]
            fn evaluation_ends_within_the_budget(expr in expr()) {
                let contexts = &*RICH_CONTEXTS;
                let mut evaluator = eval::Evaluator::new(contexts);
                let _ = evaluator.eval(&expr);
                prop_assert!(evaluator.steps <= MAX_STEPS);
            }
        }
    }
}
