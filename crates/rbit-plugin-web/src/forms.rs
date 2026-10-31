//! Bounded data-only form input. Submitted values are omitted from Debug output.
use crate::MutationError;
use std::{collections::BTreeMap, fmt};

pub const MAX_FORM_FIELDS: usize = 32;
pub const MAX_FORM_BODY_BYTES: usize = 32 * 1024;
pub const MAX_FORM_FIELD_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldKind {
    Text { max_bytes: usize, multiline: bool },
    Integer { min: i64, max: i64 },
    Boolean,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub name: String,
    pub kind: FormFieldKind,
    pub required: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSchema(Vec<FormField>);
impl FormSchema {
    pub fn new(fields: Vec<FormField>) -> Result<Self, MutationError> {
        if fields.len() > MAX_FORM_FIELDS {
            return Err(MutationError::Invalid("form field count"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for field in &fields {
            if !valid_field_name(&field.name) || !seen.insert(&field.name) {
                return Err(MutationError::Invalid("invalid or duplicate form field"));
            }
            match field.kind {
                FormFieldKind::Text { max_bytes, .. } if max_bytes == 0 || max_bytes > MAX_FORM_FIELD_BYTES => {
                    return Err(MutationError::Invalid("text field limit"));
                }
                FormFieldKind::Integer { min, max } if min > max => return Err(MutationError::Invalid("integer range")),
                _ => {}
            }
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &[FormField] {
        &self.0
    }
    pub fn validate(&self, mut input: FormData) -> Result<FormValues, MutationError> {
        if input.0.keys().any(|name| !self.0.iter().any(|field| &field.name == name)) {
            return Err(MutationError::InvalidForm);
        }
        let mut values = BTreeMap::new();
        for field in &self.0 {
            let Some(value) = input.0.remove(&field.name) else {
                if field.required {
                    return Err(MutationError::InvalidForm);
                }
                continue;
            };
            let value = match field.kind {
                FormFieldKind::Text { max_bytes, multiline } => {
                    if value.len() > max_bytes
                        || (field.required && value.is_empty())
                        || value.chars().any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
                    {
                        return Err(MutationError::InvalidForm);
                    }
                    FormValue::Text(value)
                }
                FormFieldKind::Integer { min, max } => {
                    let number = value.parse::<i64>().map_err(|_| MutationError::InvalidForm)?;
                    if !(min..=max).contains(&number) {
                        return Err(MutationError::InvalidForm);
                    }
                    FormValue::Integer(number)
                }
                FormFieldKind::Boolean => match value.as_str() {
                    "true" => FormValue::Boolean(true),
                    "false" => FormValue::Boolean(false),
                    _ => return Err(MutationError::InvalidForm),
                },
            };
            values.insert(field.name.clone(), value);
        }
        Ok(FormValues(values))
    }
}
fn valid_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// The host decodes URL-encoded input only after applying its encoded body limit.
/// Reserved host fields (such as `_csrf`) must be verified and removed first.
#[derive(Clone)]
pub struct FormData(BTreeMap<String, String>);
impl FormData {
    pub fn new(fields: Vec<(String, String)>) -> Result<Self, MutationError> {
        if fields.len() > MAX_FORM_FIELDS {
            return Err(MutationError::InvalidForm);
        }
        let mut total = 0_usize;
        let mut values = BTreeMap::new();
        for (name, value) in fields {
            total = total.checked_add(name.len()).and_then(|n| n.checked_add(value.len())).ok_or(MutationError::InvalidForm)?;
            if !valid_field_name(&name)
                || value.len() > MAX_FORM_FIELD_BYTES
                || total > MAX_FORM_BODY_BYTES
                || value.contains('\0')
                || values.insert(name, value).is_some()
            {
                return Err(MutationError::InvalidForm);
            }
        }
        Ok(Self(values))
    }
}
#[derive(Clone)]
pub enum FormValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
}
#[derive(Clone)]
pub struct FormValues(BTreeMap<String, FormValue>);
impl FormValues {
    pub fn get(&self, name: &str) -> Option<&FormValue> {
        self.0.get(name)
    }
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(FormValue::Text(value)) => Some(value),
            _ => None,
        }
    }
    pub fn integer(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(FormValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }
    pub fn boolean(&self, name: &str) -> Option<bool> {
        match self.get(name) {
            Some(FormValue::Boolean(value)) => Some(*value),
            _ => None,
        }
    }
}
impl fmt::Debug for FormData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormData").field("fields", &self.0.len()).finish_non_exhaustive()
    }
}
impl fmt::Debug for FormValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormValues").field("fields", &self.0.len()).finish_non_exhaustive()
    }
}
impl fmt::Debug for FormValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FormValue([redacted])")
    }
}

/// An explicit same-owner/scope POST action requested by a compiled GET page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormReference {
    pub name: String,
    pub path: String,
}
impl FormReference {
    pub fn new(name: &str, path: &str) -> Result<Self, crate::PageError> {
        if !valid_field_name(name) || !crate::validate_page_path(path) {
            return Err(crate::PageError::Invalid("form reference"));
        }
        Ok(Self { name: name.into(), path: path.into() })
    }
}
#[derive(Clone)]
pub struct FormAction {
    name: String,
    action: String,
    csrf: String,
}
impl FormAction {
    /// Host-only data: callbacks receive the proof, never its signing/session secret.
    pub fn new(name: &str, action: &str, csrf: &str) -> Result<Self, crate::PageError> {
        if !valid_field_name(name)
            || action.len() > 512
            || !action.starts_with('/')
            || action.starts_with("//")
            || action.contains(['?', '#', '%', '\\'])
            || action.chars().any(char::is_control)
            || !valid_action(action)
            || csrf.is_empty()
            || csrf.len() > 256
            || !csrf.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(crate::PageError::Invalid("host form action"));
        }
        Ok(Self { name: name.into(), action: action.into(), csrf: csrf.into() })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    pub fn csrf(&self) -> &str {
        &self.csrf
    }
}
impl fmt::Debug for FormAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormAction").field("name", &self.name).field("action", &self.action).finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, Default)]
pub struct FormActions(Vec<FormAction>);
impl FormActions {
    pub fn new(actions: Vec<FormAction>) -> Result<Self, crate::PageError> {
        if actions.len() > 8 || actions.iter().enumerate().any(|(i, a)| actions[..i].iter().any(|old| old.name == a.name)) {
            return Err(crate::PageError::Invalid("form action count or duplicates"));
        }
        Ok(Self(actions))
    }
    pub fn get(&self, name: &str) -> Option<&FormAction> {
        self.0.iter().find(|action| action.name == name)
    }
}

fn valid_action(action: &str) -> bool {
    let parts: Vec<_> = action.split('/').collect();
    let tail = if parts.get(1) == Some(&"-") && parts.get(2) == Some(&"plugins") {
        &parts[3..]
    } else if parts.len() > 6
        && parts[3] == "-"
        && parts[4] == "plugins"
        && crate::pages::valid_name(parts[1], 39)
        && crate::pages::valid_name(parts[2], 100)
    {
        &parts[5..]
    } else {
        return false;
    };
    tail.len() > 1 && tail[0].parse::<rbit_plugin_api::PluginId>().is_ok() && crate::validate_page_path(&tail[1..].join("/"))
}
