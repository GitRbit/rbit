//! Spec permissions contracts and original validation.
use crate::model::{Permission, Permissions};
use serde::{Deserialize, Serialize};

/// The coarse token permissions of §11.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPermissions {
    pub contents: Access,
    pub issues: Access,
    pub pull_requests: Access,
}

/// One coarse permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    None,
    Read,
    Write,
}

impl From<Permission> for Access {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::None => Self::None,
            Permission::Read => Self::Read,
            Permission::Write => Self::Write,
        }
    }
}

impl From<Permissions> for TokenPermissions {
    fn from(permissions: Permissions) -> Self {
        Self { contents: permissions.contents.into(), issues: permissions.issues.into(), pull_requests: permissions.pull_requests.into() }
    }
}
