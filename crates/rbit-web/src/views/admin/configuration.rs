//! Typed, section-scoped administrator forms. JSON stays an internal persistence format.
pub mod field;
mod schema;

use std::collections::{BTreeMap, BTreeSet};

use field::Field;
use rbit_core::config::ProviderConfig;
use rbit_localization::settings::config::{ConfigMessage, ConfigText};
use rbit_localization::{Locale, Localized};
use rbit_services::ServiceError;
use serde_json::Value;

const MAX_PROVIDERS: usize = 32;

#[derive(Debug, Clone, Copy)]
enum Kind {
    Text,
    Number,
    Toggle,
    Lines,
    Secret,
    Choice(&'static [&'static str]),
}

#[derive(Debug)]
struct Spec {
    key: &'static str,
    kind: Kind,
    optional: bool,
    max: &'static str,
}

#[derive(Debug)]
struct Section {
    key: &'static str,
    fields: &'static [Spec],
}

#[derive(Debug)]
pub struct SectionLink {
    pub key: &'static str,
    pub title: Localized,
    pub active: bool,
}

#[derive(Debug)]
pub struct Provider {
    pub index: usize,
    pub title: String,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct ConfigurationForm {
    pub section: &'static str,
    pub title: Localized,
    pub help: Localized,
    pub sections: Vec<SectionLink>,
    pub fields: Vec<Field>,
    pub providers: Vec<Provider>,
}

impl ConfigurationForm {
    #[must_use]
    pub fn with_inputs(mut self, inputs: &BTreeMap<String, String>) -> Self {
        for field in self.fields.iter_mut().chain(self.providers.iter_mut().flat_map(|provider| &mut provider.fields)) {
            if inputs.contains_key(&format!("clear.{}", field.name)) {
                field.secret_action = "clear";
            }
        }
        self
    }

    pub fn new(document: &Value, key: &str, locale: Locale) -> Result<Self, ServiceError> {
        let section = section(key)?;
        let fields = Field::list(section.fields, (key, ""), &document[key], key == "storage", locale);
        let untitled = ConfigMessage::NewProvider.text(&locale).to_string();
        let providers = document[key].get("providers").and_then(Value::as_array).map_or_else(Vec::new, |providers| {
            providers
                .iter()
                .enumerate()
                .map(|(index, provider)| Provider {
                    index,
                    title: provider["name"].as_str().filter(|name| !name.is_empty()).unwrap_or(&untitled).into(),
                    fields: Field::list(schema::PROVIDER, ("provider", &format!("providers.{index}.")), provider, false, locale),
                })
                .collect()
        });
        Ok(Self {
            section: section.key,
            title: ConfigText::SectionTitle(key).text(&locale),
            help: ConfigText::SectionHelp(key).text(&locale),
            fields,
            providers,
            sections: schema::SECTIONS
                .iter()
                .map(|item| SectionLink { key: item.key, title: ConfigText::SectionTitle(item.key).text(&locale), active: item.key == key })
                .collect(),
        })
    }
}

fn section(key: &str) -> Result<&'static Section, ServiceError> {
    schema::SECTIONS.iter().find(|section| section.key == key).ok_or(ServiceError::NotFound)
}

/// Applies only the displayed section, preserving all other settings. Draft changes for
/// adding/removing providers never reach persistence until the administrator saves.
pub fn apply_submission(document: &mut Value, key: &str, inputs: &BTreeMap<String, String>) -> Result<bool, ServiceError> {
    let section = section(key)?;
    if key == "storage" {
        return Err(invalid("Storage must be relocated offline."));
    }
    let mut allowed: BTreeSet<String> = ["section", "action"].into_iter().map(str::to_owned).collect();
    let mut first_error = None;
    apply_fields(&mut document[key], (key, section.fields), "", inputs, &mut allowed, &mut first_error);
    if key == "auth" {
        allowed.insert("provider_count".into());
        let count = inputs
            .get("provider_count")
            .and_then(|count| count.parse::<usize>().ok())
            .filter(|&count| count <= MAX_PROVIDERS)
            .ok_or_else(|| invalid("Invalid number of sign-in providers (maximum 32)."))?;
        let providers = document[key]["providers"].as_array_mut().ok_or_else(|| invalid("Invalid sign-in provider settings."))?;
        providers.resize_with(count, || serde_json::to_value(ProviderConfig::default()).expect("provider configuration serializes"));
        for (index, provider) in providers.iter_mut().enumerate() {
            apply_fields(provider, ("provider", schema::PROVIDER), &format!("providers.{index}."), inputs, &mut allowed, &mut first_error);
        }
    }
    if inputs.keys().any(|key| !allowed.contains(key)) {
        return Err(invalid("The form contains an unknown configuration field."));
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    match inputs.get("action").map_or("save", String::as_str) {
        "save" => Ok(true),
        "add_provider" if key == "auth" => {
            let providers = document[key]["providers"].as_array_mut().expect("providers were checked");
            if providers.len() >= MAX_PROVIDERS {
                return Err(invalid("You can configure at most 32 sign-in providers."));
            }
            providers.push(serde_json::to_value(ProviderConfig::default()).expect("provider configuration serializes"));
            Ok(false)
        }
        action if key == "auth" && action.starts_with("remove_provider:") => {
            let index =
                action.trim_start_matches("remove_provider:").parse::<usize>().map_err(|_| invalid("Choose a provider to remove."))?;
            let providers = document[key]["providers"].as_array_mut().expect("providers were checked");
            if index >= providers.len() {
                return Err(invalid("That provider no longer exists."));
            }
            providers.remove(index);
            Ok(false)
        }
        _ => Err(invalid("Choose a configuration action from the form.")),
    }
}

fn apply_fields(
    target: &mut Value,
    (section, specs): (&str, &[Spec]),
    prefix: &str,
    inputs: &BTreeMap<String, String>,
    allowed: &mut BTreeSet<String>,
    first_error: &mut Option<ServiceError>,
) {
    for spec in specs {
        let name = format!("{prefix}{}", spec.key);
        allowed.insert(name.clone());
        if matches!(spec.kind, Kind::Secret) {
            allowed.insert(format!("clear.{name}"));
        }
        if spec.optional && matches!(spec.kind, Kind::Lines) {
            allowed.insert(format!("default.{name}"));
        }
        match parse_field(spec, section, &name, &target[spec.key], inputs) {
            Ok(value) => target[spec.key] = value,
            Err(error) => {
                // Keep non-secret input for correction; secret fields are always rendered empty.
                if let Some(raw) = inputs.get(&name) {
                    target[spec.key] = Value::String(raw.clone());
                }
                first_error.get_or_insert(error);
            }
        }
    }
}

fn parse_field(spec: &Spec, section: &str, name: &str, prior: &Value, inputs: &BTreeMap<String, String>) -> Result<Value, ServiceError> {
    let label = ConfigText::Label { section, key: spec.key }.text(&Locale::English);
    if matches!(spec.kind, Kind::Toggle) {
        return match inputs.get(name).map(String::as_str) {
            None => Ok(Value::Bool(false)),
            Some("true") => Ok(Value::Bool(true)),
            _ => Err(invalid(&format!("{label}: choose enabled or disabled."))),
        };
    }
    let raw = inputs.get(name).ok_or_else(|| invalid(&format!("{label} is missing from the form.")))?;
    if matches!(spec.kind, Kind::Secret) {
        if inputs.contains_key(&format!("clear.{name}")) {
            if !raw.is_empty() {
                return Err(invalid(&format!("{label}: enter a replacement or clear the credential, not both.")));
            }
            return Ok(if spec.optional { Value::Null } else { Value::String(String::new()) });
        }
        return Ok(if raw.is_empty() { prior.clone() } else { Value::String(raw.clone()) });
    }
    if spec.optional && matches!(spec.kind, Kind::Lines) && inputs.contains_key(&format!("default.{name}")) {
        return Ok(Value::Null);
    }
    if spec.optional && raw.trim().is_empty() && !matches!(spec.kind, Kind::Lines) {
        return Ok(Value::Null);
    }
    match spec.kind {
        Kind::Number => raw
            .parse::<u64>()
            .ok()
            .filter(|value| spec.max.parse::<u64>().is_ok_and(|max| *value <= max))
            .map(|value| Value::Number(value.into()))
            .ok_or_else(|| invalid(&format!("{label} must be a whole number between 0 and {}.", spec.max))),
        Kind::Lines => {
            Ok(Value::Array(raw.lines().map(str::trim).filter(|line| !line.is_empty()).map(|line| Value::String(line.into())).collect()))
        }
        Kind::Choice(choices) => {
            if !choices.contains(&raw.as_str()) {
                return Err(invalid(&format!("{label}: choose one of the offered options.")));
            }
            Ok(if spec.key == "allow_registration" && raw != "always" { Value::Bool(raw == "true") } else { Value::String(raw.clone()) })
        }
        _ => Ok(Value::String(raw.clone())),
    }
}

fn invalid(message: &str) -> ServiceError {
    ServiceError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::Config;
    use serde_json::json;

    fn build(document: &Value, key: &str) -> ConfigurationForm {
        ConfigurationForm::new(document, key, Locale::English).unwrap()
    }

    fn document() -> Value {
        let mut config = Config::default();
        config.auth.providers.push(ProviderConfig { id: "company".into(), name: "Company SSO".into(), ..ProviderConfig::default() });
        serde_json::to_value(config).unwrap()
    }

    fn inputs(form: &ConfigurationForm) -> BTreeMap<String, String> {
        let mut values = BTreeMap::from([("section".into(), form.section.into()), ("action".into(), "save".into())]);
        for field in form.fields.iter().chain(form.providers.iter().flat_map(|provider| &provider.fields)) {
            if field.kind != "checkbox" || field.value == "true" {
                values.insert(field.name.clone(), field.value.clone());
            }
            if field.kind == "lines" && field.optional && field.default_mode == "default" {
                values.insert(format!("default.{}", field.name), "true".into());
            }
        }
        if form.section == "auth" {
            values.insert("provider_count".into(), form.providers.len().to_string());
        }
        values
    }

    #[test]
    fn every_configuration_field_has_a_control_and_round_trips_without_changes() {
        let original = document();
        let sections: BTreeSet<_> = schema::SECTIONS.iter().map(|section| section.key).collect();
        assert_eq!(sections, original.as_object().unwrap().keys().map(String::as_str).collect());
        for section in schema::SECTIONS {
            let mut keys: BTreeSet<_> = section.fields.iter().map(|field| field.key).collect();
            if section.key == "auth" {
                keys.insert("providers");
            }
            assert_eq!(keys, original[section.key].as_object().unwrap().keys().map(String::as_str).collect(), "{}", section.key);
            if section.key == "storage" {
                continue;
            }
            let form = build(&original, section.key);
            let mut copy = original.clone();
            assert!(apply_submission(&mut copy, section.key, &inputs(&form)).unwrap());
            assert_eq!(copy, original, "{}", section.key);
        }
        let provider_keys: BTreeSet<_> = schema::PROVIDER.iter().map(|field| field.key).collect();
        assert_eq!(provider_keys, original["auth"]["providers"][0].as_object().unwrap().keys().map(String::as_str).collect());
    }

    #[test]
    fn secrets_are_never_rendered_and_can_be_preserved_replaced_or_cleared() {
        let mut doc = document();
        doc["mail"]["smtp_password"] = json!("<unchanged>");
        let form = build(&doc, "mail");
        let secret = form.fields.iter().find(|field| field.name == "smtp_password").unwrap();
        assert!(secret.value.is_empty() && secret.configured);
        let mut values = inputs(&form);
        apply_submission(&mut doc, "mail", &values).unwrap();
        assert_eq!(doc["mail"]["smtp_password"], "<unchanged>");
        values.insert("smtp_password".into(), "new secret with spaces".into());
        apply_submission(&mut doc, "mail", &values).unwrap();
        assert_eq!(doc["mail"]["smtp_password"], "new secret with spaces");
        assert_eq!(build(&doc, "mail").fields.iter().find(|field| field.kind == "password").unwrap().value, "");
        values.insert("clear.smtp_password".into(), "true".into());
        assert!(apply_submission(&mut doc, "mail", &values).is_err());
        values.insert("smtp_password".into(), String::new());
        apply_submission(&mut doc, "mail", &values).unwrap();
        assert_eq!(doc["mail"]["smtp_password"], "");
    }

    #[test]
    fn fields_validate_types_preserve_other_sections_and_retain_failed_input() {
        let mut doc = document();
        let original = doc.clone();
        let mut values = inputs(&build(&doc, "mail"));
        values.insert("smtp_port".into(), "65536".into());
        values.insert("from_name".into(), "New sender".into());
        assert!(apply_submission(&mut doc, "mail", &values).is_err());
        let form = build(&doc, "mail");
        assert_eq!(form.fields.iter().find(|field| field.name == "from_name").unwrap().value, "New sender");
        assert_eq!(doc["repository"], original["repository"]);
        values.insert("smtp_port".into(), String::new());
        values.insert("transport".into(), "made-up".into());
        assert!(apply_submission(&mut doc, "mail", &values).is_err());
        values.insert("transport".into(), "disabled".into());
        values.insert("repository.default_branch".into(), "hijacked".into());
        assert!(apply_submission(&mut doc, "mail", &values).is_err());
        assert!(apply_submission(&mut doc, "storage", &BTreeMap::new()).is_err());
        assert!(ConfigurationForm::new(&doc, "unknown", Locale::English).is_err());
    }

    #[test]
    fn unchecked_toggles_disable_features_and_network_lists_preserve_entries() {
        let mut doc = document();
        let mut values = inputs(&build(&doc, "webhooks"));
        values.remove("enabled");
        apply_submission(&mut doc, "webhooks", &values).unwrap();
        assert_eq!(doc["webhooks"]["enabled"], false);
        values = inputs(&build(&doc, "security"));
        values.insert("trusted_proxies".into(), "127.0.0.1\r\n 10.0.0.0/8 \n".into());
        apply_submission(&mut doc, "security", &values).unwrap();
        assert_eq!(doc["security"]["trusted_proxies"], json!(["127.0.0.1", "10.0.0.0/8"]));
        assert!(serde_json::from_value::<Config>(doc).is_ok());
    }

    #[test]
    fn provider_cards_add_remove_and_handle_optional_lists_and_registration_choices() {
        let mut doc = document();
        let mut values = inputs(&build(&doc, "auth"));
        values.insert("action".into(), "add_provider".into());
        assert!(!apply_submission(&mut doc, "auth", &values).unwrap());
        assert_eq!(doc["auth"]["providers"].as_array().unwrap().len(), 2);
        values = inputs(&build(&doc, "auth"));
        values.insert("providers.0.allow_registration".into(), "always".into());
        values.insert("providers.0.scopes".into(), "openid\n email \n\nprofile".into());
        values.remove("default.providers.0.scopes");
        values.insert("action".into(), "remove_provider:1".into());
        assert!(!apply_submission(&mut doc, "auth", &values).unwrap());
        assert_eq!(doc["auth"]["providers"][0]["scopes"], json!(["openid", "email", "profile"]));
        assert_eq!(doc["auth"]["providers"][0]["allow_registration"], "always");
        assert_eq!(doc["auth"]["providers"].as_array().unwrap().len(), 1);
        values = inputs(&build(&doc, "auth"));
        values.insert("providers.0.scopes".into(), String::new());
        values.insert("providers.0.allow_registration".into(), "false".into());
        assert!(apply_submission(&mut doc, "auth", &values).unwrap());
        assert_eq!(doc["auth"]["providers"][0]["scopes"], json!([]));
        assert_eq!(doc["auth"]["providers"][0]["allow_registration"], false);
    }
}
