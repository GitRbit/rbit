//! Common account insertion keeps legacy SQL shape and optionally captures its epoch.
use super::NewUser;
use crate::{StorageError, sqlite::pool::WriteTransaction};
use rbit_core::{Account, AccountId, AccountKind, Timestamp, User};
use rbit_identity_models::{accounts::CreatedAccountSnapshot, credential::CredentialEpoch};

macro_rules! login_insert {
    ($tail:literal) => {
        concat!("INSERT INTO user_logins (account_id, email, password_hash, is_admin, email_verified_at) VALUES (?, ?, ?, ?, ?)", $tail)
    };
}
struct InsertedAccount {
    user: User,
    epoch: Option<CredentialEpoch>,
}

pub(in crate::sqlite) async fn insert_user(
    tx: &mut WriteTransaction,
    new: NewUser,
    registration_open: Option<bool>,
    email_verified: bool,
) -> Result<User, StorageError> {
    Ok(insert_account(tx, new, registration_open, email_verified, false).await?.user)
}
pub(in crate::sqlite) async fn insert_user_original(
    tx: &mut WriteTransaction,
    new: NewUser,
    registration_open: Option<bool>,
    email_verified: bool,
) -> Result<CreatedAccountSnapshot, StorageError> {
    let created = insert_account(tx, new, registration_open, email_verified, true).await?;
    let epoch = created.epoch.ok_or_else(|| StorageError::Corrupt("missing creation epoch".into()))?;
    Ok(CreatedAccountSnapshot { user: created.user, epoch })
}

/// Shared by password and external registration so the bootstrap decision is serialized
/// across both paths. The first INSERT acquires `SQLite`'s write lock before we inspect
/// the logins. No password hashing or other slow work runs while holding the lock.
async fn insert_account(
    tx: &mut WriteTransaction,
    mut new: NewUser,
    registration_open: Option<bool>,
    email_verified: bool,
    capture_epoch: bool,
) -> Result<InsertedAccount, StorageError> {
    let now = Timestamp::now();
    let id: i64 =
        sqlx::query_scalar("INSERT INTO accounts (kind, name, display_name, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id")
            .bind(AccountKind::User.as_str())
            .bind(new.name.as_str())
            .bind(new.display_name.as_deref())
            .bind(now.0)
            .bind(now.0)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| StorageError::from_insert(e, "username"))?;

    if let Some(open) = registration_open {
        let first: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM user_logins)").fetch_one(&mut **tx).await?;
        if !open && !first {
            return Err(StorageError::RegistrationClosed);
        }
        new.is_admin = first;
    }
    // Allocation is part of this SAME insertion transaction, after the original
    // bootstrap decision. No provisional epoch escapes an unsuccessful commit.
    let epoch = if capture_epoch { Some(super::super::credential_mutations::allocate(tx).await?) } else { None };
    let query = sqlx::query(if capture_epoch {
        sqlx::SqlStr::from_static("INSERT INTO user_logins (account_id, email, password_hash, is_admin, email_verified_at, authentication_epoch) VALUES (?, ?, ?, ?, ?, ?)")
    } else {
        sqlx::SqlStr::from_static(login_insert!(""))
    })
    .bind(id)
    .bind(new.email.as_str())
    .bind(&new.password_hash)
    .bind(new.is_admin)
    .bind(email_verified.then_some(now.0));
    let query = if let Some(epoch) = epoch { query.bind(epoch.get()) } else { query };
    query.execute(&mut **tx).await.map_err(|error| StorageError::from_insert(error, "email"))?;

    Ok(InsertedAccount {
        user: User {
            account: Account {
                id: AccountId(id),
                kind: AccountKind::User,
                name: new.name,
                display_name: new.display_name,
                bio: None,
                created_at: now,
            },
            email: new.email,
            is_admin: new.is_admin,
        },
        epoch,
    })
}
