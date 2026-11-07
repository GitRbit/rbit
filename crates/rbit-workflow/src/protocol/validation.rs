//! Protocol validation contracts and original validation.
use super::limits::{MAX_LABEL_CHARS, MAX_OUTPUT_BYTES, MAX_OUTPUT_KEY_BYTES, MAX_RUNNER_LABELS};
use std::collections::BTreeMap;
use std::fmt;

/// Why a message was refused. The message names the field and is safe to return to the
/// other side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// `Err` with `message` unless `condition` holds.
pub(crate) fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result<(), ValidationError> {
    if condition { Ok(()) } else { Err(ValidationError::new(message())) }
}

/// `runs-on` labels: 1 to [`crate::protocol::MAX_RUNNER_LABELS`] of 1 to [`crate::protocol::MAX_LABEL_CHARS`] `[A-Za-z0-9._-]`.
pub(crate) fn validate_labels<'a>(labels: impl ExactSizeIterator<Item = &'a str>) -> Result<(), ValidationError> {
    ensure((1..=MAX_RUNNER_LABELS).contains(&labels.len()), || format!("there must be 1 to {MAX_RUNNER_LABELS} labels"))?;
    for label in labels {
        ensure(
            !label.is_empty()
                && label.len() <= MAX_LABEL_CHARS
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
            || format!("label `{label}` must be 1 to {MAX_LABEL_CHARS} letters, digits, '.', '_' or '-'"),
        )?;
    }
    Ok(())
}

pub(crate) fn unique<T: Ord>(items: impl Iterator<Item = T>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    items.into_iter().all(|item| seen.insert(item))
}

/// Output names are `[A-Za-z_][A-Za-z0-9_-]*` of at most [`crate::protocol::MAX_OUTPUT_KEY_BYTES`]; names and
/// values together are at most [`crate::protocol::MAX_OUTPUT_BYTES`].
pub(crate) fn validate_outputs<V: AsRef<str>>(outputs: &BTreeMap<String, V>) -> Result<(), ValidationError> {
    let mut total = 0usize;
    for (name, value) in outputs {
        ensure(name.len() <= MAX_OUTPUT_KEY_BYTES && valid_identifier(name), || {
            format!("output name `{name}` must start with a letter or '_' and contain only letters, digits, '_' and '-'")
        })?;
        total = total.saturating_add(name.len()).saturating_add(value.as_ref().len());
    }
    ensure(total <= MAX_OUTPUT_BYTES, || format!("outputs may be at most {MAX_OUTPUT_BYTES} bytes"))
}

/// `[A-Za-z_][A-Za-z0-9_-]*`, the shape of output names.
fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
