//! Scoped tokens (agent plan step 4.8): a personal access token limited to one repository
//! and to some rights within its scope. An agent's token pushes and merges in its own
//! repository and does nothing else; the owner lists, creates and revokes it over the API.
//!
//! A token without a limit is unchanged: its scope decides, everywhere.

use rbit_core::{AccessToken, AccessTokenId, RepoId, User};
use rbit_identity::access_tokens::TokenAuthentication;
use rbit_storage::sqlite::StoredLimit;

use super::{AccessTokenService, CreateAccessToken, map};
use crate::ServiceError;

/// One right a limited token may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Right {
    /// Push with Git.
    Push,
    /// Publish commit statuses.
    Statuses,
    /// Merge, queue, update branches, revert.
    Merge,
    /// Run workflows by hand.
    Dispatch,
    /// Approve and rerun workflow runs.
    Approve,
    /// Read run logs and artifacts.
    ReadLogs,
}

impl Right {
    pub const ALL: [Self; 6] = [Self::Push, Self::Statuses, Self::Merge, Self::Dispatch, Self::Approve, Self::ReadLogs];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Statuses => "statuses",
            Self::Merge => "merge",
            Self::Dispatch => "dispatch",
            Self::Approve => "approve",
            Self::ReadLogs => "read_logs",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|right| right.as_str() == name)
    }
}

/// What a limited token may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLimit {
    /// The one repository it may reach; any its owner may when `None`.
    pub repo: Option<RepoId>,
    pub rights: Vec<Right>,
}

impl TokenLimit {
    #[must_use]
    pub fn allows(&self, right: Right) -> bool {
        self.rights.contains(&right)
    }

    #[must_use]
    pub fn reaches(&self, repo: RepoId) -> bool {
        self.repo.is_none_or(|limited| limited == repo)
    }
}

fn from_stored(stored: StoredLimit) -> TokenLimit {
    TokenLimit { repo: stored.repo, rights: stored.rights.into_iter().filter_map(|name| Right::parse(&name)).collect() }
}

impl AccessTokenService {
    /// [`Self::create`] with `limit` stored beside the token. A limit that cannot be stored
    /// revokes the token again, so no unlimited token is left behind.
    pub async fn create_limited(&self, owner: &User, input: CreateAccessToken, limit: &TokenLimit) -> Result<(AccessToken, String), ServiceError> {
        let (token, secret) = self.create(owner, input).await?;
        let stored = StoredLimit { repo: limit.repo, rights: limit.rights.iter().map(|right| right.as_str().to_owned()).collect() };
        if let Err(error) = self.store.set_limit(token.id, &stored).await {
            let _ = self.revoke(owner, token.id).await;
            return Err(error.into());
        }
        tracing::info!(user = %owner.name(), token = token.id.0, repo = ?limit.repo, "access token limited");
        Ok((token, secret))
    }

    /// The limit of token `id`; `None` for a token without one.
    pub async fn limit_of(&self, id: AccessTokenId) -> Result<Option<TokenLimit>, ServiceError> {
        Ok(self.store.limit_of(id).await?.map(from_stored))
    }

    /// [`Self::authenticate`] with the token's limit.
    pub async fn authenticate_limited(&self, secret: &str) -> Result<Option<(User, rbit_core::TokenScope, Option<TokenLimit>)>, ServiceError> {
        match self.application().authenticate(secret).await.map_err(map)? {
            TokenAuthentication::Accepted { user, scope, token, bookkeeping } => {
                if let Err(error) = bookkeeping {
                    tracing::warn!(%error, token = token.0, "failed to record access token use");
                }
                let limit = self.limit_of(token).await?;
                Ok(Some((user, scope, limit)))
            }
            TokenAuthentication::SuspendedOwner { user } => {
                tracing::info!(user = %user.name(), "suspended user's access token refused");
                Ok(None)
            }
            TokenAuthentication::Refused => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rights_round_trip_and_limits_reach_one_repository() {
        for right in Right::ALL {
            assert_eq!(Right::parse(right.as_str()), Some(right));
        }
        assert_eq!(Right::parse("admin"), None);
        let limit = TokenLimit { repo: Some(RepoId(6)), rights: vec![Right::Push, Right::Merge] };
        assert!(limit.reaches(RepoId(6)) && !limit.reaches(RepoId(7)));
        assert!(limit.allows(Right::Merge) && !limit.allows(Right::Statuses));
        assert!(TokenLimit { repo: None, rights: Vec::new() }.reaches(RepoId(7)));
    }
}
