//! Accounts: the owners of repositories. Users and organisations share one namespace,
//! so `/{name}` resolves unambiguously to either kind.

use serde::{Deserialize, Serialize};

use crate::{AccountName, Email, Timestamp};

/// Stable, never-reused primary key of an account. Used for on-disk paths so that
/// renaming an account never moves files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountKind {
    User,
    Organization,
}

impl AccountKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Organization => "organization",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "organization" => Some(Self::Organization),
            _ => None,
        }
    }
}

/// Public profile information shared by users and organisations.
#[derive(Debug, Clone)]
pub struct Account {
    pub id: AccountId,
    pub kind: AccountKind,
    pub name: AccountName,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub created_at: Timestamp,
}

impl Account {
    /// The name to show in the UI: the display name when set, otherwise the handle.
    pub fn label(&self) -> &str {
        self.display_name.as_deref().filter(|n| !n.is_empty()).unwrap_or(self.name.as_str())
    }

    pub const fn is_organization(&self) -> bool {
        matches!(self.kind, AccountKind::Organization)
    }
}

/// An account that can sign in.
#[derive(Debug, Clone)]
pub struct User {
    pub account: Account,
    pub email: Email,
    pub is_admin: bool,
}

impl User {
    pub fn id(&self) -> AccountId {
        self.account.id
    }

    pub fn name(&self) -> &AccountName {
        &self.account.name
    }
}
