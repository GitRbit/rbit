//! Restart-required configuration, with redacted secrets in the administrator editor.
//!
//! The document rules (redaction, restoring secrets, staging checks, validation) live in
//! `rbit_core::config::document` and opening the stored document in
//! `rbit_storage::deployment_config`, shared with the server's startup loader. This
//! adapter keeps the editor working for every web host, embedded ones included: it
//! loads the pending document, seals secrets before saving and records the audit log.
use rbit_core::{
    Config, User,
    config::document::{parse_document, redact, stage},
};

use super::{AdminService, require_admin};
use crate::{
    ServiceError,
    audit::{AuditAction, AuditEntry},
};

pub use rbit_core::config::document::validate_configuration;
pub use rbit_storage::deployment_config::{DEPLOYMENT_CONFIG_KEY as CONFIGURATION_KEY, open_stored};

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
                let mut config =
                    open_stored(&value, self.ctx.database.sealing_key()).map_err(|error| ServiceError::Internal(error.into()))?;
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
        let value = parse_document(document).map_err(ServiceError::Invalid)?;
        let existing = serde_json::to_value(self.pending_configuration().await?).map_err(ServiceError::internal)?;
        let config = stage(value, &existing, &self.ctx.config.storage.data_dir).map_err(ServiceError::Invalid)?;
        let mut document = serde_json::to_value(&config).map_err(ServiceError::internal)?;
        self.ctx.database.sealing_key().seal_secret_fields(&mut document).map_err(ServiceError::internal)?;
        let document = document.to_string();
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
