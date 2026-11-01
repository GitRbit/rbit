//! Bounded data-only form input. Submitted values are omitted from Debug output.
use crate::MutationError;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
pub const MAX_FORM_FIELDS: usize = 32;
pub const MAX_FORM_BODY_BYTES: usize = 32 * 1024;
pub const MAX_FORM_FIELD_BYTES: usize = 4096;
pub const MAX_LARGE_FORM_BODY_BYTES: usize = 256 * 1024;
pub const MAX_LARGE_FORM_FIELD_BYTES: usize = 64 * 1024;
pub const MAX_FORM_LIST_ITEMS: usize = 128;
pub const MAX_FORM_PAIRS: usize = 256;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldKind {
    Text { max_bytes: usize, multiline: bool },
    Integer { min: i64, max: i64 },
    Boolean,
    TextList { max_items: usize, max_bytes: usize, multiline: bool },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub name: String,
    pub kind: FormFieldKind,
    pub required: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormLimits {
    encoded_bytes: usize,
    field_bytes: usize,
    pairs: usize,
}
impl FormLimits {
    pub const fn max_encoded_bytes(self) -> usize {
        self.encoded_bytes
    }
    pub const fn max_field_bytes(self) -> usize {
        self.field_bytes
    }
    pub const fn max_pairs(self) -> usize {
        self.pairs
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prefix {
    start: String,
    end: String,
    max_entries: usize,
    max_bytes: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSchema {
    fields: Vec<FormField>,
    large: bool,
    prefix: Option<Prefix>,
}
impl FormSchema {
    pub fn new(fields: Vec<FormField>) -> Result<Self, MutationError> {
        Self::build(fields, false)
    }
    pub fn large_text(fields: Vec<FormField>) -> Result<Self, MutationError> {
        Self::build(fields, true)
    }
    fn build(fields: Vec<FormField>, large: bool) -> Result<Self, MutationError> {
        if fields.len() > MAX_FORM_FIELDS {
            return Err(MutationError::Invalid("form field count"));
        }
        let mut seen = BTreeSet::new();
        let limit = if large { MAX_LARGE_FORM_FIELD_BYTES } else { MAX_FORM_FIELD_BYTES };
        for field in &fields {
            if !valid_field_name(&field.name) || !seen.insert(&field.name) {
                return Err(MutationError::Invalid("invalid or duplicate form field"));
            }
            match field.kind {
                FormFieldKind::Text { max_bytes, .. } | FormFieldKind::TextList { max_bytes, .. }
                    if max_bytes == 0 || max_bytes > limit =>
                {
                    return Err(MutationError::Invalid("text field limit"));
                }
                FormFieldKind::TextList { max_items, .. } if max_items == 0 || max_items > MAX_FORM_LIST_ITEMS => {
                    return Err(MutationError::Invalid("list field count"));
                }
                FormFieldKind::Integer { min, max } if min > max => return Err(MutationError::Invalid("integer range")),
                _ => {}
            }
        }
        Ok(Self { fields, large, prefix: None })
    }
    pub fn with_prefix(mut self, prefix: &str, suffix: &str, max_entries: usize, max_bytes: usize) -> Result<Self, MutationError> {
        if self.prefix.is_some()
            || !prefix.ends_with('[')
            || !prefix.strip_suffix('[').is_some_and(valid_field_name)
            || prefix.len() > 32
            || suffix != "]"
            || max_entries == 0
            || max_entries > 32
            || max_bytes == 0
            || max_bytes > self.limits().field_bytes
        {
            return Err(MutationError::Invalid("dynamic form prefix"));
        }
        self.prefix = Some(Prefix { start: prefix.into(), end: suffix.into(), max_entries, max_bytes });
        Ok(self)
    }
    pub fn fields(&self) -> &[FormField] {
        &self.fields
    }
    pub fn limits(&self) -> FormLimits {
        let pairs = self
            .fields
            .iter()
            .map(|f| match f.kind {
                FormFieldKind::TextList { max_items, .. } => max_items,
                _ => 1,
            })
            .sum::<usize>()
            + self.prefix.as_ref().map_or(0, |p| p.max_entries);
        FormLimits {
            encoded_bytes: if self.large { MAX_LARGE_FORM_BODY_BYTES } else { MAX_FORM_BODY_BYTES },
            field_bytes: if self.large { MAX_LARGE_FORM_FIELD_BYTES } else { MAX_FORM_FIELD_BYTES },
            pairs: pairs.clamp(MAX_FORM_FIELDS, MAX_FORM_PAIRS),
        }
    }
    pub fn accepts_repeated(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name && matches!(f.kind, FormFieldKind::TextList { .. }))
    }
    pub fn accepts_name(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name) || self.prefix_name(name)
    }
    fn prefix_name(&self, name: &str) -> bool {
        self.prefix.as_ref().is_some_and(|p| {
            name.strip_prefix(&p.start).and_then(|n| n.strip_suffix(&p.end)).is_some_and(|inner| {
                !inner.is_empty()
                    && inner.len() <= 64
                    && inner.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            })
        })
    }
    fn check_pairs(&self, pairs: &[(String, String)]) -> Result<(), MutationError> {
        if pairs.len() > self.limits().pairs {
            return Err(MutationError::InvalidForm);
        }
        let mut total = 0usize;
        let mut names = BTreeMap::new();
        let mut dynamic = 0usize;
        for (name, value) in pairs {
            total = total.checked_add(name.len()).and_then(|n| n.checked_add(value.len())).ok_or(MutationError::InvalidForm)?;
            if total > self.limits().encoded_bytes || value.contains('\0') {
                return Err(MutationError::InvalidForm);
            }
            let count = names.entry(name.as_str()).or_insert(0usize);
            *count += 1;
            if let Some(field) = self.fields.iter().find(|f| f.name == *name) {
                let (max_bytes, max_items) = match field.kind {
                    FormFieldKind::Text { max_bytes, .. } => (max_bytes, 1),
                    FormFieldKind::TextList { max_bytes, max_items, .. } => (max_bytes, max_items),
                    FormFieldKind::Integer { .. } => (32, 1),
                    FormFieldKind::Boolean => (5, 1),
                };
                if value.len() > max_bytes || *count > max_items {
                    return Err(MutationError::InvalidForm);
                }
            } else if self.prefix_name(name) {
                dynamic += 1;
                let prefix = self.prefix.as_ref().expect("prefix checked");
                if *count > 1 || dynamic > prefix.max_entries || value.len() > prefix.max_bytes {
                    return Err(MutationError::InvalidForm);
                }
            } else {
                return Err(MutationError::InvalidForm);
            }
        }
        Ok(())
    }
    pub fn validate(&self, input: FormData) -> Result<FormValues, MutationError> {
        self.check_pairs(&input.0)?;
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in input.0 {
            grouped.entry(name).or_default().push(value);
        }
        let mut values = BTreeMap::new();
        for field in &self.fields {
            let Some(mut given) = grouped.remove(&field.name) else {
                if field.required {
                    return Err(MutationError::InvalidForm);
                }
                continue;
            };
            let value = match field.kind {
                FormFieldKind::Text { multiline, .. } => {
                    let value = given.pop().expect("present form value");
                    check_text(&value, multiline, field.required)?;
                    FormValue::Text(value)
                }
                FormFieldKind::TextList { multiline, .. } => {
                    for value in &given {
                        check_text(value, multiline, field.required)?;
                    }
                    FormValue::TextList(given)
                }
                FormFieldKind::Integer { min, max } => {
                    let number = given[0].parse::<i64>().map_err(|_| MutationError::InvalidForm)?;
                    if !(min..=max).contains(&number) {
                        return Err(MutationError::InvalidForm);
                    }
                    FormValue::Integer(number)
                }
                FormFieldKind::Boolean => match given[0].as_str() {
                    "true" => FormValue::Boolean(true),
                    "false" => FormValue::Boolean(false),
                    _ => return Err(MutationError::InvalidForm),
                },
            };
            values.insert(field.name.clone(), value);
        }
        for (name, mut given) in grouped {
            let value = given.pop().expect("present prefix value");
            check_text(&value, false, false)?;
            values.insert(name, FormValue::Text(value));
        }
        Ok(FormValues(values))
    }
}
fn check_text(value: &str, multiline: bool, required: bool) -> Result<(), MutationError> {
    if (required && value.is_empty()) || value.chars().any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t'))) {
        Err(MutationError::InvalidForm)
    } else {
        Ok(())
    }
}
fn valid_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
#[derive(Clone)]
pub struct FormData(Vec<(String, String)>);
impl FormData {
    pub fn new(fields: Vec<(String, String)>) -> Result<Self, MutationError> {
        let mut names = BTreeSet::new();
        let mut total = 0usize;
        if fields.len() > MAX_FORM_FIELDS {
            return Err(MutationError::InvalidForm);
        }
        for (name, value) in &fields {
            total = total.checked_add(name.len()).and_then(|n| n.checked_add(value.len())).ok_or(MutationError::InvalidForm)?;
            if !valid_field_name(name)
                || value.len() > MAX_FORM_FIELD_BYTES
                || total > MAX_FORM_BODY_BYTES
                || value.contains('\0')
                || !names.insert(name)
            {
                return Err(MutationError::InvalidForm);
            }
        }
        Ok(Self(fields))
    }
    pub fn for_schema(fields: Vec<(String, String)>, schema: &FormSchema) -> Result<Self, MutationError> {
        schema.check_pairs(&fields)?;
        Ok(Self(fields))
    }
}
#[derive(Clone)]
pub enum FormValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    TextList(Vec<String>),
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
    pub fn list(&self, name: &str) -> Option<&[String]> {
        match self.get(name) {
            Some(FormValue::TextList(value)) => Some(value),
            _ => None,
        }
    }
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        self.0
            .iter()
            .flat_map(|(key, value)| match value {
                FormValue::Text(v) => vec![(key.clone(), v.clone())],
                FormValue::Integer(v) => vec![(key.clone(), v.to_string())],
                FormValue::Boolean(v) => vec![(key.clone(), v.to_string())],
                FormValue::TextList(v) => v.iter().map(|v| (key.clone(), v.clone())).collect(),
            })
            .collect()
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
pub struct FormActions {
    actions: Vec<FormAction>,
    locale_preference: Option<crate::LocaleChoice>,
}
impl FormActions {
    pub fn new(actions: Vec<FormAction>) -> Result<Self, crate::PageError> {
        if actions.len() > 32 || actions.iter().enumerate().any(|(i, a)| actions[..i].iter().any(|old| old.name == a.name)) {
            return Err(crate::PageError::Invalid("form action count or duplicates"));
        }
        Ok(Self { actions, locale_preference: None })
    }
    pub fn set_locale_preference(&mut self, choice: crate::LocaleChoice) {
        self.locale_preference = Some(choice);
    }
    pub fn locale_preference(&self) -> Option<&crate::LocaleChoice> {
        self.locale_preference.as_ref()
    }
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
    pub fn for_action(&self, action: &str) -> Option<&FormAction> {
        self.actions.iter().find(|form| form.action() == action)
    }
    pub fn get(&self, name: &str) -> Option<&FormAction> {
        self.actions.iter().find(|action| action.name == name)
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
