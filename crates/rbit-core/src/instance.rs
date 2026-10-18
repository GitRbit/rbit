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

/// Who may create organisations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrgCreationPolicy {
    #[default]
    AnyUser,
    AdminsOnly,
}

impl OrgCreationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AnyUser => "any_user",
            Self::AdminsOnly => "admins_only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "any_user" => Some(Self::AnyUser),
            "admins_only" => Some(Self::AdminsOnly),
            _ => None,
        }
    }
}

/// The effective instance settings (defaults merged with administrator overrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSettings {
    pub site_name: String,
    pub branding: crate::config::BrandingConfig,
    pub registration: RegistrationMode,
    pub org_creation: OrgCreationPolicy,
    pub default_visibility: Visibility,
}

impl InstanceSettings {
    /// The settings implied by the static configuration, before any overrides.
    pub fn defaults(config: &Config) -> Self {
        Self {
            site_name: config.server.site_name.clone(),
            branding: config.branding.clone(),
            registration: if config.server.open_registration { RegistrationMode::Open } else { RegistrationMode::Closed },
            org_creation: OrgCreationPolicy::AnyUser,
            default_visibility: Visibility::Public,
        }
    }
}
