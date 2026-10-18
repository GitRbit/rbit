//! Runtime instance settings (site name, registration, …) editable by administrators.
//!
//! The configuration supplies defaults; overrides persisted in SQLite are merged over them
//! at start-up and after every update. The effective settings are cached in memory, so
//! [`InstanceService::current`] is synchronous and cheap enough for every request.

use std::sync::{Arc, RwLock};

use rbit_core::{InstanceSettings, OrgCreationPolicy, RegistrationMode, User, Visibility};
pub use rbit_storage::sqlite::admin::PublicCounts;
use rbit_storage::sqlite::{admin::AdminStore, instance::InstanceSettingsStore};

use crate::{ServiceContext, ServiceError};

/// Bounds on the site name, which appears in every page title and the PWA manifest.
pub const SITE_NAME_MAX_LEN: usize = 50;

/// Storage keys. Renaming one orphans existing overrides, so treat them as a format.
mod key {
    pub const BRANDING: &str = "branding";
    pub const SITE_NAME: &str = "site_name";
    pub const REGISTRATION: &str = "registration";
    pub const ORG_CREATION: &str = "org_creation";
    pub const DEFAULT_VISIBILITY: &str = "default_visibility";
}

#[derive(Debug, Clone)]
pub struct InstanceService {
    store: InstanceSettingsStore,
    counts: AdminStore,
    settings: Arc<RwLock<InstanceSettings>>,
}

impl InstanceService {
    /// Loads the effective settings (config defaults merged with stored overrides).
    pub(crate) async fn load(ctx: &ServiceContext) -> Result<Self, ServiceError> {
        let store = ctx.database.instance_settings();
        let mut settings = InstanceSettings::defaults(&ctx.config);
        for (key, value) in store.load().await? {
            apply_override(&mut settings, &key, &value);
        }
        Ok(Self { store, counts: ctx.database.admin(), settings: Arc::new(RwLock::new(settings)) })
    }

    /// A snapshot of the current settings. Cheap; safe to call on every request.
    pub fn current(&self) -> InstanceSettings {
        self.settings.read().map_or_else(|poisoned| poisoned.into_inner().clone(), |s| s.clone())
    }

    /// Public repositories, active users and organisations, for the front page. One cheap
    /// query; it reveals nothing a visitor could not count by browsing.
    pub async fn public_counts(&self) -> Result<PublicCounts, ServiceError> {
        Ok(self.counts.public_counts().await?)
    }

    /// Persists `new` as the instance settings and makes it effective immediately.
    /// Site administrators only; the site name is trimmed and must be 1–50 characters.
    pub async fn update(&self, actor: &User, new: InstanceSettings) -> Result<(), ServiceError> {
        if !actor.is_admin {
            return Err(ServiceError::Forbidden);
        }
        new.branding.validate().map_err(ServiceError::Invalid)?;
        let branding = serde_json::to_string(&new.branding).map_err(ServiceError::internal)?;
        let new = InstanceSettings { site_name: validate_site_name(&new.site_name)?, ..new };

        self.store
            .save(&[
                (key::SITE_NAME, new.site_name.as_str()),
                (key::BRANDING, branding.as_str()),
                (key::REGISTRATION, new.registration.as_str()),
                (key::ORG_CREATION, new.org_creation.as_str()),
                (key::DEFAULT_VISIBILITY, new.default_visibility.as_str()),
            ])
            .await?;

        tracing::info!(
            admin = %actor.name(),
            site_name = %new.site_name,
            registration = new.registration.as_str(),
            org_creation = new.org_creation.as_str(),
            default_visibility = new.default_visibility.as_str(),
            "instance settings updated"
        );
        match self.settings.write() {
            Ok(mut current) => *current = new,
            Err(poisoned) => *poisoned.into_inner() = new,
        }
        Ok(())
    }
}

/// Trims and checks a site name: 1–50 characters, no control characters.
pub fn validate_site_name(name: &str) -> Result<String, ServiceError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 {
        return Err(ServiceError::Invalid("site name must not be empty".to_owned()));
    }
    if length > SITE_NAME_MAX_LEN {
        return Err(ServiceError::Invalid(format!("site name must be at most {SITE_NAME_MAX_LEN} characters")));
    }
    if name.chars().any(char::is_control) {
        return Err(ServiceError::Invalid("site name must not contain control characters".to_owned()));
    }
    Ok(name.to_owned())
}

/// Applies one stored override. Unknown keys and unparsable values are ignored (and
/// logged), so a bad row can never stop the instance from starting.
fn apply_override(settings: &mut InstanceSettings, key: &str, value: &str) {
    let applied = match key {
        key::BRANDING => serde_json::from_str::<rbit_core::config::BrandingConfig>(value)
            .ok()
            .filter(|branding| branding.validate().is_ok())
            .map(|branding| settings.branding = branding),
        "deployment_config" => Some(()),
        key::SITE_NAME => validate_site_name(value).ok().map(|name| settings.site_name = name),
        key::REGISTRATION => RegistrationMode::parse(value).map(|mode| settings.registration = mode),
        key::ORG_CREATION => OrgCreationPolicy::parse(value).map(|policy| settings.org_creation = policy),
        key::DEFAULT_VISIBILITY => Visibility::parse(value).map(|visibility| settings.default_visibility = visibility),
        _ => None,
    }
    .is_some();
    if !applied {
        tracing::warn!(key, value, "ignoring invalid instance setting override");
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::Config;

    use super::*;

    #[test]
    fn site_name_validation() {
        assert_eq!(validate_site_name("  Forge  ").unwrap(), "Forge");
        assert!(validate_site_name("   ").is_err());
        assert!(validate_site_name(&"x".repeat(51)).is_err());
        assert_eq!(validate_site_name(&"é".repeat(50)).unwrap().chars().count(), 50);
        assert!(validate_site_name("a\nb").is_err());
    }

    #[test]
    fn overrides_merge_over_defaults_and_ignore_garbage() {
        let mut settings = InstanceSettings::defaults(&Config::default());
        apply_override(&mut settings, "site_name", "Forge");
        apply_override(&mut settings, "registration", "closed");
        apply_override(&mut settings, "org_creation", "admins_only");
        apply_override(&mut settings, "default_visibility", "private");
        apply_override(&mut settings, "default_visibility", "secret");
        apply_override(&mut settings, "site_name", "");
        apply_override(&mut settings, "unknown", "value");

        assert_eq!(
            settings,
            InstanceSettings {
                site_name: "Forge".to_owned(),
                branding: rbit_core::config::BrandingConfig::default(),
                registration: RegistrationMode::Closed,
                org_creation: OrgCreationPolicy::AdminsOnly,
                default_visibility: Visibility::Private,
            }
        );
    }
}
