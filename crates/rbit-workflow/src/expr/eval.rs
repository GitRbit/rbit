//! Evaluating an [`Expr`] against [`Contexts`], within [`MAX_STEPS`].
//!
//! Values read from a context are borrowed, not copied, so `github.event.pull_request`
//! costs a lookup rather than a clone of the event; that is also what makes GitHub's rule
//! "objects and arrays are equal only when they are the same instance" checkable.

use std::{borrow::Cow, cmp::Ordering};

use serde_json::Value;

use super::{Accessor, Comparison, Contexts, Expr, ExprError, MAX_STEPS, fail, functions, number_value, to_number, to_text, truthy};

/// A value during evaluation: borrowed from a context, or computed.
pub(super) type Val<'c> = Cow<'c, Value>;

pub(super) struct Evaluator<'c> {
    pub(super) contexts: &'c dyn Contexts,
    /// Steps taken so far; never more than [`MAX_STEPS`].
    pub(super) steps: usize,
}

/// The result of an access chain: one value, or many once a `.*` filter was applied.
enum Selection<'c> {
    One(Val<'c>),
    Many(Vec<Val<'c>>),
}

impl<'c> Evaluator<'c> {
    pub(super) fn new(contexts: &'c dyn Contexts) -> Self {
        Self { contexts, steps: 0 }
    }

    /// Spends `count` steps of the budget.
    pub(super) fn charge(&mut self, count: usize) -> Result<(), ExprError> {
        self.steps = self.steps.saturating_add(count);
        if self.steps > MAX_STEPS {
            self.steps = MAX_STEPS;
            return fail(format!("the expression needs more than {MAX_STEPS} evaluation steps"));
        }
        Ok(())
    }

    pub(super) fn eval(&mut self, expr: &Expr) -> Result<Val<'c>, ExprError> {
        self.charge(1)?;
        Ok(match expr {
            Expr::Null => Cow::Owned(Value::Null),
            Expr::Bool(value) => Cow::Owned(Value::Bool(*value)),
            Expr::Number(value) => Cow::Owned(number_value(*value)),
            Expr::String(text) => Cow::Owned(Value::String(text.clone())),
            Expr::Context(name) => match self.contexts.context(name) {
                Some(value) => Cow::Borrowed(value),
                None => return fail(format!("the `{name}` context is not available here")),
            },
            Expr::Access(base, accessors) => {
                let mut selection = Selection::One(self.eval(base)?);
                for accessor in accessors {
                    selection = self.access(selection, accessor)?;
                }
                match selection {
                    Selection::One(value) => value,
                    Selection::Many(values) => Cow::Owned(Value::Array(values.into_iter().map(Cow::into_owned).collect())),
                }
            }
            Expr::Call(function, arguments) => functions::call(self, *function, arguments)?,
            Expr::Not(inner) => Cow::Owned(Value::Bool(!truthy(self.eval(inner)?.as_ref()))),
            Expr::Compare(op, left, right) => {
                let left = self.eval(left)?;
                let right = self.eval(right)?;
                Cow::Owned(Value::Bool(compare(*op, &left, &right)))
            }
            Expr::And(items) => self.chain(items, false)?,
            Expr::Or(items) => self.chain(items, true)?,
        })
    }

    /// `&&` (stop at the first falsy operand) or `||` (stop at the first truthy one); the
    /// result is the operand it stopped at, or the last.
    fn chain(&mut self, items: &[Expr], stop_when: bool) -> Result<Val<'c>, ExprError> {
        let mut last = Cow::Owned(Value::Null);
        for item in items {
            last = self.eval(item)?;
            if truthy(&last) == stop_when {
                break;
            }
        }
        Ok(last)
    }

    fn access(&mut self, selection: Selection<'c>, accessor: &Accessor) -> Result<Selection<'c>, ExprError> {
        let index = match accessor {
            Accessor::Index(index) => Some(self.eval(index)?),
            Accessor::Property(_) | Accessor::Filter => None,
        };
        let key = |value: &Value| -> Option<Key> {
            match (accessor, &index) {
                (Accessor::Property(name), _) => Some(Key::Name(name.clone())),
                (Accessor::Index(_), Some(index)) => match (value, index.as_ref()) {
                    (Value::Array(_), Value::Number(_)) => Some(Key::Position(to_number(index))),
                    (Value::Object(_), index) => Some(Key::Name(to_text(index))),
                    _ => None,
                },
                _ => None,
            }
        };
        Ok(match (selection, accessor) {
            (Selection::One(value), Accessor::Filter) => {
                let children = children(value);
                self.charge(children.len())?;
                Selection::Many(children)
            }
            (Selection::One(value), _) => {
                let key = key(&value);
                Selection::One(key.and_then(|key| child(value, &key)).unwrap_or(Cow::Owned(Value::Null)))
            }
            (Selection::Many(values), Accessor::Filter) => {
                let mut flattened = Vec::new();
                for value in values {
                    let children = children(value);
                    self.charge(children.len().max(1))?;
                    flattened.extend(children);
                }
                Selection::Many(flattened)
            }
            (Selection::Many(values), _) => {
                self.charge(values.len())?;
                // Elements without the property are dropped, as GitHub's object filter does.
                Selection::Many(
                    values
                        .into_iter()
                        .filter_map(|value| {
                            let key = key(&value)?;
                            child(value, &key)
                        })
                        .collect(),
                )
            }
        })
    }
}

enum Key {
    Name(String),
    Position(f64),
}

/// A property (matched case-insensitively) or an array element.
fn child<'c>(value: Val<'c>, key: &Key) -> Option<Val<'c>> {
    fn pick<'v>(value: &'v Value, key: &Key) -> Option<&'v Value> {
        match (value, key) {
            (Value::Object(object), Key::Name(name)) => {
                object.get(name).or_else(|| object.iter().find(|(candidate, _)| eq_ignore_case(candidate, name)).map(|(_, value)| value))
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "the index was checked to be in range")]
            (Value::Array(items), Key::Position(position)) if *position >= 0.0 && *position < 4_294_967_296.0 => {
                items.get(position.trunc() as usize)
            }
            _ => None,
        }
    }
    match value {
        Cow::Borrowed(value) => pick(value, key).map(Cow::Borrowed),
        Cow::Owned(value) => pick(&value, key).cloned().map(Cow::Owned),
    }
}

/// What `.*` selects from one value: an object's values or an array's items.
fn children(value: Val<'_>) -> Vec<Val<'_>> {
    match value {
        Cow::Borrowed(Value::Object(object)) => object.values().map(Cow::Borrowed).collect(),
        Cow::Borrowed(Value::Array(items)) => items.iter().map(Cow::Borrowed).collect(),
        Cow::Owned(Value::Object(object)) => object.into_iter().map(|(_, value)| Cow::Owned(value)).collect(),
        Cow::Owned(Value::Array(items)) => items.into_iter().map(Cow::Owned).collect(),
        _ => Vec::new(),
    }
}

/// Strings compare ignoring case, as GitHub does (per character, upper-cased).
pub(super) fn eq_ignore_case(a: &str, b: &str) -> bool {
    a == b || a.chars().flat_map(char::to_uppercase).eq(b.chars().flat_map(char::to_uppercase))
}

fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
    a.chars().flat_map(char::to_uppercase).cmp(b.chars().flat_map(char::to_uppercase))
}

/// GitHub's `==`: values of the same kind compare directly (strings ignoring case; arrays
/// and objects only as the same instance); values of different kinds compare as numbers,
/// where `NaN` equals nothing.
pub(super) fn loose_eq(left: &Val<'_>, right: &Val<'_>) -> bool {
    match (left.as_ref(), right.as_ref()) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => eq_ignore_case(a, b),
        (Value::Array(_), Value::Array(_)) | (Value::Object(_), Value::Object(_)) => match (left, right) {
            (Cow::Borrowed(a), Cow::Borrowed(b)) => std::ptr::eq(*a, *b),
            _ => false,
        },
        _ => to_number(left) == to_number(right),
    }
}

/// The order of two values for `<`, `<=`, `>` and `>=`, under the same coercions as
/// [`loose_eq`]; `None` when they are not ordered (`NaN`, arrays, objects).
fn order(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::String(a), Value::String(b)) => Some(cmp_ignore_case(a, b)),
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => None,
        _ => to_number(left).partial_cmp(&to_number(right)),
    }
}

fn compare(op: Comparison, left: &Val<'_>, right: &Val<'_>) -> bool {
    match op {
        Comparison::Equal => loose_eq(left, right),
        Comparison::NotEqual => !loose_eq(left, right),
        Comparison::Less => order(left, right) == Some(Ordering::Less),
        Comparison::LessEqual => matches!(order(left, right), Some(Ordering::Less | Ordering::Equal)),
        Comparison::Greater => order(left, right) == Some(Ordering::Greater),
        Comparison::GreaterEqual => matches!(order(left, right), Some(Ordering::Greater | Ordering::Equal)),
    }
}
