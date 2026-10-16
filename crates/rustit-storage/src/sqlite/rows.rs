//! Raw row shapes and their conversion into validated domain types.

use rustit_core::{
    AccessToken, AccessTokenId, Account, AccountId, AccountKind, Email, RepoId, Repository, Timestamp, TokenScope, User, Visibility,
    repository::OwnedRepository,
};

use crate::StorageError;

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct AccountRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub created_at: i64,
}

impl TryFrom<AccountRow> for Account {
    type Error = StorageError;

    fn try_from(row: AccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: AccountId(row.id),
            kind: AccountKind::parse(&row.kind).ok_or_else(|| corrupt("account kind", &row.kind))?,
            name: row.name.parse().map_err(|e| corrupt("account name", e))?,
            display_name: row.display_name,
            bio: row.bio,
            created_at: Timestamp(row.created_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct UserRow {
    #[sqlx(flatten)]
    pub account: AccountRow,
    pub email: String,
    pub is_admin: bool,
    pub password_hash: String,
}

impl UserRow {
    /// Splits the row into the domain user and the stored password hash.
    pub fn into_user_and_hash(self) -> Result<(User, String), StorageError> {
        let email: Email = self.email.parse().map_err(|e| corrupt("email", e))?;
        let user = User { account: self.account.try_into()?, email, is_admin: self.is_admin };
        Ok((user, self.password_hash))
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RepositoryRow {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub default_branch: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub pushed_at: Option<i64>,
}

impl TryFrom<RepositoryRow> for Repository {
    type Error = StorageError;

    fn try_from(row: RepositoryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: RepoId(row.id),
            owner_id: AccountId(row.owner_id),
            name: row.name.parse().map_err(|e| corrupt("repository name", e))?,
            description: row.description,
            visibility: Visibility::parse(&row.visibility).ok_or_else(|| corrupt("visibility", &row.visibility))?,
            default_branch: row.default_branch,
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
            pushed_at: row.pushed_at.map(Timestamp),
        })
    }
}

/// A repository joined with its owning account (columns prefixed `owner_`).
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct OwnedRepositoryRow {
    #[sqlx(flatten)]
    pub repo: RepositoryRow,
    pub owner_kind: String,
    pub owner_name: String,
    pub owner_display_name: Option<String>,
    pub owner_bio: Option<String>,
    pub owner_created_at: i64,
}

impl TryFrom<OwnedRepositoryRow> for OwnedRepository {
    type Error = StorageError;

    fn try_from(row: OwnedRepositoryRow) -> Result<Self, Self::Error> {
        let owner = AccountRow {
            id: row.repo.owner_id,
            kind: row.owner_kind,
            name: row.owner_name,
            display_name: row.owner_display_name,
            bio: row.owner_bio,
            created_at: row.owner_created_at,
        };
        Ok(Self { owner: owner.try_into()?, repo: row.repo.try_into()? })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct AccessTokenRow {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub hint: String,
    pub scope: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
}

impl TryFrom<AccessTokenRow> for AccessToken {
    type Error = StorageError;

    fn try_from(row: AccessTokenRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: AccessTokenId(row.id),
            account_id: AccountId(row.account_id),
            name: row.name,
            hint: row.hint,
            scope: TokenScope::parse(&row.scope).ok_or_else(|| corrupt("token scope", &row.scope))?,
            created_at: Timestamp(row.created_at),
            last_used_at: row.last_used_at.map(Timestamp),
            expires_at: row.expires_at.map(Timestamp),
        })
    }
}

/// An access token joined with its owning user (token columns prefixed `token_`).
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct AccessTokenOwnerRow {
    #[sqlx(flatten)]
    pub user: UserRow,
    pub token_id: i64,
    pub token_name: String,
    pub token_hint: String,
    pub token_scope: String,
    pub token_created_at: i64,
    pub token_last_used_at: Option<i64>,
    pub token_expires_at: Option<i64>,
}

impl AccessTokenOwnerRow {
    /// Splits the row into the token and its owner, discarding the owner's password hash.
    pub fn into_token_and_user(self) -> Result<(AccessToken, User), StorageError> {
        let token = AccessTokenRow {
            id: self.token_id,
            account_id: self.user.account.id,
            name: self.token_name,
            hint: self.token_hint,
            scope: self.token_scope,
            created_at: self.token_created_at,
            last_used_at: self.token_last_used_at,
            expires_at: self.token_expires_at,
        };
        let (user, _password_hash) = self.user.into_user_and_hash()?;
        Ok((token.try_into()?, user))
    }
}
