//! Instance-wide settings that administrators change at runtime.
//!
//! [`crate::Config`] provides the defaults; overrides are persisted by the storage layer
//! and served from memory by the instance service.

use crate::{Config, Visibility};

/// Who may create an account from the sign-up page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegistrationMode {
    #[default]
    Open,
    /// Only administrators can create accounts (the very first account is always allowed).
    Closed,
}

impl RegistrationMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Creation policy ownership lives in `rbit-access`; this path stays for existing callers.
pub use rbit_access::OrgCreationPolicy;

/// The effective instance settings (defaults merged with administrator overrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSettings {
    pub site_name: String,
    pub branding: crate::config::BrandingConfig,
    pub registration: RegistrationMode,
    /// Whether `/explore` (and the links to it) are offered. When off, it is `404` for everyone.
    pub explore_enabled: bool,
    pub org_creation: OrgCreationPolicy,
    pub default_visibility: Visibility,
    /// The hour (0–23, UTC) email digests are sent at for users who did not choose one.
    pub digest_hour: u8,
}

impl InstanceSettings {
    /// The settings implied by the static configuration, before any overrides.
    pub fn defaults(config: &Config) -> Self {
        Self {
            site_name: config.server.site_name.clone(),
            branding: config.branding.clone(),
            registration: if config.server.open_registration { RegistrationMode::Open } else { RegistrationMode::Closed },
            explore_enabled: config.server.explore_enabled,
            org_creation: OrgCreationPolicy::AnyUser,
            default_visibility: Visibility::Public,
            digest_hour: config.notifications.digest_hour,
        }
    }
}

/// Longest site name, in characters.
pub const SITE_NAME_MAX_LEN: usize = 50;

/// Trims and checks a site name: 1–50 characters, no control characters.
pub fn checked_site_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 {
        return Err("site name must not be empty".to_owned());
    }
    if length > SITE_NAME_MAX_LEN {
        return Err(format!("site name must be at most {SITE_NAME_MAX_LEN} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err("site name must not contain control characters".to_owned());
    }
    Ok(name.to_owned())
}
