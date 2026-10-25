//! Restart-required configuration, with redacted secrets in the administrator editor.
use rbit_core::{Config, User};
use serde_json::{Value, json};

use super::{AdminService, require_admin};
use crate::{
    ServiceError,
    audit::{AuditAction, AuditEntry},
};

pub const CONFIGURATION_KEY: &str = "deployment_config";
const REDACTED: &str = "<unchanged>";

impl AdminService {
    pub async fn configuration(&self, actor: &User) -> Result<String, ServiceError> {
        require_admin(actor)?;
        let config = self.pending_configuration().await?;
        let mut value = serde_json::to_value(config).map_err(ServiceError::internal)?;
        redact(&mut value);
        serde_json::to_string_pretty(&value).map_err(ServiceError::internal)
    }

    async fn pending_configuration(&self) -> Result<Config, ServiceError> {
        for (key, value) in self.ctx.database.instance_settings().load().await? {
            if key == CONFIGURATION_KEY {
                let mut config: Config = serde_json::from_str(&value).map_err(ServiceError::internal)?;
                // Storage is deployment-owned. After an offline move, a saved snapshot
                // may still contain the old path; it must not block subsequent edits.
                config.storage.clone_from(&self.ctx.config.storage);
                return Ok(config);
            }
        }
        Ok((*self.ctx.config).clone())
    }

    /// Saves a complete configuration for the next process start. Storage relocation
    /// must be done offline; changing it here could silently start an empty instance.
    pub async fn stage_configuration(&self, actor: &User, document: &str) -> Result<(), ServiceError> {
        require_admin(actor)?;
        if document.len() > 128 * 1024 {
            return Err(ServiceError::Invalid("configuration must be at most 128 KiB".into()));
        }
        let mut value: Value =
            serde_json::from_str(document).map_err(|_| ServiceError::Invalid("configuration must be valid JSON".into()))?;
        let existing = serde_json::to_value(self.pending_configuration().await?).map_err(ServiceError::internal)?;
        let object = value.as_object().ok_or_else(|| ServiceError::Invalid("configuration must be a JSON object".into()))?;
        if existing.as_object().is_none_or(|current| current.keys().any(|key| !object.contains_key(key))) {
            return Err(ServiceError::Invalid("keep every configuration section in the document".into()));
        }
        restore_secrets(&mut value, &existing)?;
        let config: Config =
            serde_json::from_value(value).map_err(|_| ServiceError::Invalid("unknown configuration field or invalid value type".into()))?;
        validate_configuration(&config).map_err(ServiceError::Invalid)?;
        if config.storage.data_dir != self.ctx.config.storage.data_dir {
            return Err(ServiceError::Invalid("move storage offline and update the deployment file or environment".into()));
        }
        let document = serde_json::to_string(&config).map_err(ServiceError::internal)?;
        self.ctx.database.instance_settings().save(&[(CONFIGURATION_KEY, document.as_str())]).await?;
        self.audit
            .record(
                AuditEntry::new(AuditAction::InstanceSettingsChanged)
                    .by(actor)
                    .detail("deployment configuration staged; restart required (values omitted)"),
            )
            .await;
        Ok(())
    }

    pub async fn reset_configuration(&self, actor: &User) -> Result<(), ServiceError> {
        require_admin(actor)?;
        self.ctx.database.instance_settings().delete(CONFIGURATION_KEY).await?;
        self.audit
            .record(
                AuditEntry::new(AuditAction::InstanceSettingsChanged)
                    .by(actor)
                    .detail("deployment configuration overrides removed; restart required"),
            )
            .await;
        Ok(())
    }
}

/// Shared with the startup loader: malformed settings fail before services start.
pub fn validate_configuration(config: &Config) -> Result<(), String> {
    config.auth.validate()?;
    config.branding.validate()?;
    crate::instance::validate_site_name(&config.server.site_name).map_err(|e| e.to_string())?;
    let url = reqwest::Url::parse(&config.server.public_url).map_err(|_| "public URL must be an absolute HTTP or HTTPS URL")?;
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
    if config.mail.transport == rbit_core::config::MailTransport::Smtp && config.mail.smtp_host.trim().is_empty() {
        return Err("SMTP transport requires an SMTP host".into());
    }
    // Actions: foundation (F).
    config.actions.validate()?;
    Ok(())
}

fn is_secret(key: &str) -> bool {
    key.ends_with("password") || key.ends_with("secret") || key.ends_with("token")
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_secret(key) && value.as_str().is_some_and(|s| !s.is_empty()) {
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

fn restore_secrets(value: &mut Value, existing: &Value) -> Result<(), ServiceError> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_secret(key) && value.as_str() == Some(REDACTED) {
                    *value = existing
                        .get(key)
                        .cloned()
                        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                        .ok_or_else(|| ServiceError::Invalid("enter a secret for newly added credentials".into()))?;
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
}
