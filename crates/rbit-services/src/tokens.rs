//! Personal access tokens: the service adapter over the identity token application.
//!
//! Validation, limits, secret handling and credential checks live in
//! `rbit_identity::access_tokens`. This adapter maps errors, logs, and records creation
//! and revocation in the security audit log.

use rbit_core::{AccessToken, AccessTokenId, TokenScope, User};
use rbit_identity::access_tokens::{self, AccessTokenApplication, AccessTokenError, TokenAuthentication};
use rbit_storage::{AccessTokenStore, AccountStore, StorageError};

use crate::{
    ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

// Agent loop: scoped tokens.
mod limits;
pub use limits::{Right, TokenLimit};

/// Raw "new token" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct CreateAccessToken {
    pub name: String,
    pub scope: TokenScope,
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AccessTokenService {
    store: AccessTokenStore,
    accounts: AccountStore,
    audit: AuditService,
}

impl AccessTokenService {
    /// Allowed values for `expires_in_days` (None = never). The UI renders these as choices.
    pub const EXPIRY_CHOICES: &'static [Option<u32>] = access_tokens::EXPIRY_CHOICES;

    pub(crate) fn new(store: AccessTokenStore, accounts: AccountStore) -> Self {
        let audit = AuditService::from_store(accounts.audit_log());
        Self { store, accounts, audit }
    }

    fn application(&self) -> AccessTokenApplication<'_, AccessTokenStore, AccountStore> {
        AccessTokenApplication::new(&self.store, &self.accounts)
    }

    /// Returns the stored metadata and the plaintext secret (shown to the user exactly once).
    pub async fn create(&self, owner: &User, input: CreateAccessToken) -> Result<(AccessToken, String), ServiceError> {
        let (token, secret) = self.application().create(owner, &input.name, input.scope, input.expires_in_days).await.map_err(map)?;
        tracing::info!(user = %owner.name(), token = token.id.0, scope = token.scope.as_str(), "access token created");
        let detail = format!("{} scope", token.scope.as_str());
        self.audit.record(AuditEntry::new(AuditAction::TokenCreated).by(owner).on(token.name.clone()).detail(detail)).await;
        Ok((token, secret))
    }

    /// The owner's tokens, newest first.
    pub async fn list(&self, owner: &User) -> Result<Vec<AccessToken>, ServiceError> {
        self.application().list(owner).await.map_err(map)
    }

    /// [`ServiceError::NotFound`] if the token doesn't exist or isn't the owner's.
    pub async fn revoke(&self, owner: &User, id: AccessTokenId) -> Result<(), ServiceError> {
        let name = self.application().revoke(owner, id).await.map_err(map)?;
        tracing::info!(user = %owner.name(), token = id.0, "access token revoked");
        let entry = AuditEntry::new(AuditAction::TokenRevoked).by(owner);
        self.audit
            .record(match name {
                Some(name) => entry.on(name),
                None => entry,
            })
            .await;
        Ok(())
    }

    /// Resolves a Git/API credential. `Ok(None)` for unknown, malformed or expired tokens
    /// and for tokens whose owner is suspended. Records last use.
    pub async fn authenticate(&self, secret: &str) -> Result<Option<(User, TokenScope)>, ServiceError> {
        match self.application().authenticate(secret).await.map_err(map)? {
            TokenAuthentication::Accepted { user, scope, token, bookkeeping } => {
                // Bookkeeping only: a busy database must not turn a valid credential into a failure.
                if let Err(error) = bookkeeping {
                    tracing::warn!(%error, token = token.0, "failed to record access token use");
                }
                Ok(Some((user, scope)))
            }
            TokenAuthentication::SuspendedOwner { user } => {
                tracing::info!(user = %user.name(), "suspended user's access token refused");
                Ok(None)
            }
            TokenAuthentication::Refused => Ok(None),
        }
    }
}

fn map(error: AccessTokenError<StorageError, StorageError>) -> ServiceError {
    match error {
        AccessTokenError::Token(error) | AccessTokenError::Account(error) => error.into(),
        AccessTokenError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
        AccessTokenError::NotFound => ServiceError::NotFound,
    }
}
