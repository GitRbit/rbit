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

mod contexts;
mod eval;
mod functions;
mod lex;
mod parse;
mod syntax;
mod templates;
mod values;

pub use self::contexts::{ContextMap, Contexts, Status};
pub use self::syntax::{Accessor, Comparison, Expr, Function};
pub use self::templates::{Condition, Template};
pub use self::values::{format_number, number_value, to_number, to_text, truthy};
pub use serde_json::Value;
use std::fmt;

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

/// Parses and evaluates `template` in one go (see [`Template::evaluate`]).
pub fn evaluate(template: &str, contexts: &dyn Contexts) -> Result<Value, ExprError> {
    Template::parse(template)?.evaluate(contexts)
}

/// Parses and renders `template` in one go (see [`Template::render`]).
pub fn render(template: &str, contexts: &dyn Contexts) -> Result<String, ExprError> {
    Template::parse(template)?.render(contexts)
}

#[cfg(test)]
mod tests;
