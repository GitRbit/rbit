//! The deployment configuration document an administrator stages for the next start.
//!
//! Shared by every host that edits it (the service editor, used by embedded web hosts and
//! the server alike) and by the server's startup loader. Secret fields are shown redacted
//! and keep their stored value unless replaced; provider secrets follow provider IDs, so
//! reordering providers cannot swap credentials. Storage location is deployment-owned and
//! never changes through the document. Sealing the secrets is the storage layer's job.

use serde_json::{Value, json};

use super::{Config, MailTransport};

/// What a redacted secret reads as in the editor.
pub const REDACTED: &str = "<unchanged>";
/// Largest accepted document.
pub const MAX_DOCUMENT_BYTES: usize = 128 * 1024;

/// Whether a field holds a credential: its name ends in `password`, `secret` or `token`.
pub fn is_secret_field(name: &str) -> bool {
    name.ends_with("password") || name.ends_with("secret") || name.ends_with("token")
}

/// Rejects malformed settings, so a staged document can never stop the next start.
pub fn validate_configuration(config: &Config) -> Result<(), String> {
    config.auth.validate()?;
    config.branding.validate()?;
    config.security.password_job_capacity()?;
    crate::instance::checked_site_name(&config.server.site_name)?;
    let url = url::Url::parse(&config.server.public_url).map_err(|_| "public URL must be an absolute HTTP or HTTPS URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("public URL must be an HTTP or HTTPS origin, without credentials, path, query or fragment".into());
    }
    if config.repository.default_branch.trim().is_empty()
        || config.security.min_password_length == 0
        || config.security.session_lifetime_days == 0
    {
        return Err("default branch, minimum password length and session lifetime must not be empty or zero".into());
    }
    if config.mail.transport == MailTransport::Smtp && config.mail.smtp_host.trim().is_empty() {
        return Err("SMTP transport requires an SMTP host".into());
    }
    // Actions: foundation (F).
    config.actions.validate()?;
    Ok(())
}

/// The submitted document, bounded and parsed.
pub fn parse_document(document: &str) -> Result<Value, String> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err("configuration must be at most 128 KiB".into());
    }
    serde_json::from_str(document).map_err(|_| "configuration must be valid JSON".into())
}

/// The configuration a submitted document stages, given the pending one (`existing`, as
/// JSON) and the running deployment's data directory. Every section must be present,
/// redacted secrets keep their stored values and storage may not move.
pub fn stage(mut value: Value, existing: &Value, data_dir: &std::path::Path) -> Result<Config, String> {
    let object = value.as_object().ok_or("configuration must be a JSON object")?;
    if existing.as_object().is_none_or(|current| current.keys().any(|key| !object.contains_key(key))) {
        return Err("keep every configuration section in the document".into());
    }
    restore_secrets(&mut value, existing)?;
    let config: Config = serde_json::from_value(value).map_err(|_| "unknown configuration field or invalid value type")?;
    validate_configuration(&config)?;
    if config.storage.data_dir != data_dir {
        return Err("move storage offline and update the deployment file or environment".into());
    }
    Ok(config)
}

/// Replaces every non-empty secret with [`REDACTED`].
pub fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_secret_field(key) && value.as_str().is_some_and(|s| !s.is_empty()) {
                    *value = json!(REDACTED);
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact(value);
            }
        }
        _ => {}
    }
}

/// Puts the stored secret back wherever the document still says [`REDACTED`].
pub fn restore_secrets(value: &mut Value, existing: &Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_secret_field(key) && value.as_str() == Some(REDACTED) {
                    *value = existing
                        .get(key)
                        .cloned()
                        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                        .ok_or("enter a secret for newly added credentials")?;
                } else {
                    restore_secrets(value, &existing[key])?;
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter_mut().enumerate() {
                // Provider secrets follow the provider ID, so reordering cannot swap credentials.
                let prior = if let Some(id) = value.get("id") {
                    existing.as_array().and_then(|items| items.iter().find(|item| item.get("id") == Some(id))).unwrap_or(&Value::Null)
                } else {
                    &existing[index]
                };
                restore_secrets(value, prior)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_settings_are_validated() {
        let mut config = Config::default();
        assert_eq!(validate_configuration(&config), Ok(()));
        config.actions.evaluation_concurrency = 0;
        assert!(validate_configuration(&config).is_err());
    }

    #[test]
    fn provider_credentials_follow_ids_when_reordered() {
        let existing = json!({"providers": [
            {"id": "alpha", "client_secret": "alpha-secret"}, {"id": "beta", "client_secret": "beta-secret"}
        ]});
        let mut reordered = json!({"providers": [
            {"id": "beta", "client_secret": REDACTED}, {"id": "alpha", "client_secret": REDACTED}
        ]});
        restore_secrets(&mut reordered, &existing).unwrap();
        assert_eq!(reordered["providers"][0]["client_secret"], "beta-secret");
        assert_eq!(reordered["providers"][1]["client_secret"], "alpha-secret");
        let mut new_provider = json!({"providers": [{"id": "new", "client_secret": REDACTED}]});
        assert!(restore_secrets(&mut new_provider, &existing).is_err());
    }

    #[test]
    fn staging_keeps_sections_and_storage() {
        let config = Config::default();
        let existing = serde_json::to_value(&config).unwrap();
        assert!(stage(existing.clone(), &existing, &config.storage.data_dir).is_ok());
        assert_eq!(stage(json!([]), &existing, &config.storage.data_dir).unwrap_err(), "configuration must be a JSON object");
        assert_eq!(stage(json!({}), &existing, &config.storage.data_dir).unwrap_err(), "keep every configuration section in the document");
        let moved = stage(existing.clone(), &existing, std::path::Path::new("/elsewhere")).unwrap_err();
        assert_eq!(moved, "move storage offline and update the deployment file or environment");
        assert_eq!(parse_document("{x").unwrap_err(), "configuration must be valid JSON");
        assert_eq!(parse_document(&" ".repeat(MAX_DOCUMENT_BYTES + 1)).unwrap_err(), "configuration must be at most 128 KiB");
    }
}
