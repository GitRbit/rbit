//! Personal access tokens: creation, listing, revocation and Git credential checks.

use rustit_auth::AccessTokenSecret;
use rustit_core::{AccessToken, AccessTokenId, Timestamp, TokenScope, User};
use rustit_storage::{AccessTokenStore, AccountStore, NewAccessToken};

use crate::ServiceError;

const MAX_NAME_LEN: usize = 64;
const MAX_TOKENS_PER_USER: i64 = 50;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

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
}

impl AccessTokenService {
    /// Allowed values for `expires_in_days` (None = never). The UI renders these as choices.
    pub const EXPIRY_CHOICES: &'static [Option<u32>] = &[Some(7), Some(30), Some(90), Some(365), None];

    pub(crate) fn new(store: AccessTokenStore, accounts: AccountStore) -> Self {
        Self { store, accounts }
    }

    /// Returns the stored metadata and the plaintext secret (shown to the user exactly once).
    pub async fn create(&self, owner: &User, input: CreateAccessToken) -> Result<(AccessToken, String), ServiceError> {
        let name = input.name.trim();
        let name_len = name.chars().count();
        if name_len == 0 {
            return Err(ServiceError::Invalid("token name must not be empty".to_owned()));
        }
        if name_len > MAX_NAME_LEN {
            return Err(ServiceError::Invalid(format!("token name must be at most {MAX_NAME_LEN} characters")));
        }
        if !Self::EXPIRY_CHOICES.contains(&input.expires_in_days) {
            return Err(ServiceError::Invalid("choose one of the offered expiration periods".to_owned()));
        }
        if self.store.count_for_account(owner.id()).await? >= MAX_TOKENS_PER_USER {
            return Err(ServiceError::Invalid(format!(
                "you can have at most {MAX_TOKENS_PER_USER} access tokens; revoke one you no longer use"
            )));
        }

        let secret = AccessTokenSecret::generate();
        let expires_at = input.expires_in_days.map(|days| Timestamp(Timestamp::now().seconds() + i64::from(days) * SECONDS_PER_DAY));
        let token = self
            .store
            .create(NewAccessToken {
                account_id: owner.id(),
                name: name.to_owned(),
                digest: secret.digest(),
                hint: secret.hint(),
                scope: input.scope,
                expires_at,
            })
            .await?;

        tracing::info!(user = %owner.name(), token = token.id.0, scope = token.scope.as_str(), "access token created");
        Ok((token, secret.encode()))
    }

    /// The owner's tokens, newest first.
    pub async fn list(&self, owner: &User) -> Result<Vec<AccessToken>, ServiceError> {
        Ok(self.store.list_for_account(owner.id()).await?)
    }

    /// [`ServiceError::NotFound`] if the token doesn't exist or isn't the owner's.
    pub async fn revoke(&self, owner: &User, id: AccessTokenId) -> Result<(), ServiceError> {
        if !self.store.delete(owner.id(), id).await? {
            return Err(ServiceError::NotFound);
        }
        tracing::info!(user = %owner.name(), token = id.0, "access token revoked");
        Ok(())
    }

    /// Resolves a Git/API credential. `Ok(None)` for unknown, malformed or expired tokens
    /// and for tokens whose owner is suspended. Records last use.
    pub async fn authenticate(&self, secret: &str) -> Result<Option<(User, TokenScope)>, ServiceError> {
        let Some(secret) = AccessTokenSecret::parse(secret) else {
            return Ok(None);
        };
        let Some((token, user)) = self.store.find_by_digest(&secret.digest()).await? else {
            return Ok(None);
        };
        if token.is_expired() {
            return Ok(None);
        }
        if self.accounts.is_suspended(user.id()).await? {
            tracing::info!(user = %user.name(), "suspended user's access token refused");
            return Ok(None);
        }

        // Bookkeeping only: a busy database must not turn a valid credential into a failure.
        if let Err(error) = self.store.touch(token.id, Timestamp::now()).await {
            tracing::warn!(%error, token = token.id.0, "failed to record access token use");
        }
        Ok(Some((user, token.scope)))
    }
}
