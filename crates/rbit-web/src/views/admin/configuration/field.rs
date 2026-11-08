//! One rendered form control, with its catalog text in the page's language.
use rbit_localization::{Locale, Localized, settings::config::ConfigText};
use serde_json::Value;

use super::{Kind, Spec};

#[derive(Debug)]
pub struct Choice {
    pub value: &'static str,
    pub label: Localized,
    pub selected: bool,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub label: Localized,
    pub help: Localized,
    pub kind: &'static str,
    pub value: String,
    pub choices: Vec<Choice>,
    pub optional: bool,
    pub configured: bool,
    pub read_only: bool,
    pub max: &'static str,
    pub default_mode: &'static str,
    pub secret_action: &'static str,
}

impl Field {
    /// The controls of `specs` in schema `section`, named `{prefix}{key}`, with values from `values`.
    pub(super) fn list(specs: &[Spec], (section, prefix): (&str, &str), values: &Value, read_only: bool, locale: Locale) -> Vec<Self> {
        specs.iter().map(|spec| Self::new(spec, section, format!("{prefix}{}", spec.key), &values[spec.key], read_only, locale)).collect()
    }

    fn new(spec: &Spec, section: &str, name: String, value: &Value, read_only: bool, locale: Locale) -> Self {
        let text = match value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Array(values) => values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"),
            other => other.to_string(),
        };
        let (kind, choices) = match spec.kind {
            Kind::Text => ("text", Vec::new()),
            Kind::Number => ("number", Vec::new()),
            Kind::Toggle => ("checkbox", Vec::new()),
            Kind::Lines => ("lines", Vec::new()),
            Kind::Secret => ("password", Vec::new()),
            Kind::Choice(options) => {
                let label = |value| ConfigText::Choice { section, key: spec.key, value }.text(&locale);
                ("select", options.iter().map(|&value| Choice { value, label: label(value), selected: text == value }).collect())
            }
        };
        Self {
            name,
            label: ConfigText::Label { section, key: spec.key }.text(&locale),
            help: ConfigText::Help { section, key: spec.key }.text(&locale),
            kind,
            value: if matches!(spec.kind, Kind::Secret) { String::new() } else { text },
            choices,
            optional: spec.optional,
            configured: matches!(spec.kind, Kind::Secret) && value.as_str() == Some("<unchanged>"),
            read_only,
            max: spec.max,
            default_mode: if value.is_null() { "default" } else { "custom" },
            secret_action: "keep",
        }
    }
}
