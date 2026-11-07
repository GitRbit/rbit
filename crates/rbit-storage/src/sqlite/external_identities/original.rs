//! First-acceptance snapshots; legacy SQL remains unchanged and no PHC is read.
use super::{ExternalIdentity, ExternalIdentityStore, IdentityRow};
use crate::{
    StorageError,
    sqlite::rows::{AccountRow, UserRow},
};
use rbit_core::{AccountId, User};
use rbit_identity_models::{
    accounts::{LinkedProviderSnapshot, ProviderAccountSnapshot, ProviderLoginRepository},
    credential::{CredentialEpoch, CredentialFence},
};

// All aliases differ from IdentityRow's fields. LEFT JOIN preserves known-subject
// errors: a missing login/account must NEVER become unknown-subject registration.
macro_rules! principal_columns {
    () => {
        "a.id AS principal_id, a.kind AS principal_kind, a.name AS principal_name, \
        a.display_name AS principal_display_name, a.bio AS principal_bio, a.created_at AS principal_created_at, \
        l.email AS principal_email, l.is_admin AS principal_admin, l.authentication_epoch, \
        EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id=l.account_id \
        AND r.target_epoch=l.authentication_epoch AND r.completed_at IS NULL) AS pending "
    };
}
#[derive(sqlx::FromRow)]
struct PrincipalRow {
    principal_id: Option<i64>,
    principal_kind: Option<String>,
    principal_name: Option<String>,
    principal_display_name: Option<String>,
    principal_bio: Option<String>,
    principal_created_at: Option<i64>,
    principal_email: Option<String>,
    principal_admin: Option<bool>,
    authentication_epoch: Option<i64>,
    pending: bool,
}
impl PrincipalRow {
    fn snapshot(self) -> Result<ProviderAccountSnapshot, StorageError> {
        let missing = || StorageError::Corrupt("provider identity has no complete account/login".into());
        let epoch = CredentialEpoch::new(self.authentication_epoch.ok_or_else(missing)?)
            .ok_or_else(|| StorageError::Corrupt("invalid provider credential epoch".into()))?;
        let row = UserRow {
            account: AccountRow {
                id: self.principal_id.ok_or_else(missing)?,
                kind: self.principal_kind.ok_or_else(missing)?,
                name: self.principal_name.ok_or_else(missing)?,
                display_name: self.principal_display_name,
                bio: self.principal_bio,
                created_at: self.principal_created_at.ok_or_else(missing)?,
            },
            email: self.principal_email.ok_or_else(missing)?,
            is_admin: self.principal_admin.ok_or_else(missing)?,
            password_hash: String::new(),
        };
        let (user, _) = row.into_user_and_hash()?;
        Ok(ProviderAccountSnapshot { user, fence: CredentialFence { epoch, pending: self.pending } })
    }
}
#[derive(sqlx::FromRow)]
struct LinkedRow {
    #[sqlx(flatten)]
    identity: IdentityRow,
    #[sqlx(flatten)]
    principal: PrincipalRow,
}
impl ProviderLoginRepository for ExternalIdentityStore {
    type Error = StorageError;
    type Identity = ExternalIdentity;
    async fn find_linked_snapshot(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<LinkedProviderSnapshot<Self::Identity>>, Self::Error> {
        let row: Option<LinkedRow> = sqlx::query_as(concat!(
            "SELECT e.id, e.account_id, e.provider_id, e.subject, e.email, e.username, e.created_at, e.last_used_at, ",
            principal_columns!(),
            "FROM external_identities e LEFT JOIN user_logins l ON l.account_id=e.account_id \
            LEFT JOIN accounts a ON a.id=e.account_id WHERE e.provider_id=? AND e.subject=?"
        ))
        .bind(provider)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let account = row.principal.snapshot()?;
            if account.user.id().0 != row.identity.account_id {
                return Err(StorageError::Corrupt("provider account binding mismatch".into()));
            }
            Ok(LinkedProviderSnapshot { identity: row.identity.into(), account })
        })
        .transpose()
    }
    async fn find_verified_email_snapshot(&self, email: &str) -> Result<Option<ProviderAccountSnapshot>, Self::Error> {
        let row: Option<PrincipalRow> = sqlx::query_as(concat!(
            "SELECT ",
            principal_columns!(),
            "FROM user_logins l LEFT JOIN accounts a ON a.id=l.account_id WHERE l.email=? AND l.email_verified_at IS NOT NULL"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        row.map(PrincipalRow::snapshot).transpose()
    }
    async fn find_active_at_epoch(&self, account: AccountId, original: CredentialEpoch) -> Result<Option<User>, Self::Error> {
        Ok(super::super::accounts::AccountStore::new(self.pool.clone())
            .find_session_user_at_epoch(account, false, original)
            .await?
            .map(|row| row.user))
    }
}
