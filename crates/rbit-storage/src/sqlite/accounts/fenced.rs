//! Alternate, inactive one-statement credential-fence reads; legacy APIs stay unchanged.
use super::{AccountStore, LoginRecord, SessionUser, SessionUserRow, UserRow};
use crate::StorageError;
use rbit_core::AccountId;
use rbit_identity_models::credential::{CredentialEpoch, CredentialFence, CredentialLoginSnapshot};

const LOGIN_SNAPSHOT: &str = "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, \
    l.email, l.is_admin, l.password_hash, l.suspended_at, l.authentication_epoch, \
    EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id = a.id \
    AND r.target_epoch = l.authentication_epoch AND r.completed_at IS NULL) AS pending \
    FROM accounts a JOIN user_logins l ON l.account_id = a.id WHERE a.name = ?1 OR l.email = ?1";

const FENCED_SESSION_USER: &str = "SELECT a.id, a.kind, a.name, a.display_name, a.bio, a.created_at, \
    l.email, l.is_admin, '' AS password_hash, l.password_change_required, \
    CASE WHEN ?2 THEN EXISTS (SELECT 1 FROM notifications n WHERE n.user_id = a.id AND n.unread = 1) ELSE 0 END \
    AS has_unread_notifications, CASE COALESCE(l.interface_locale, l.locale) WHEN 'en' THEN 1 WHEN 'es' THEN 2 WHEN 'zh-Hans' THEN 3 \
    WHEN 'pt-BR' THEN 4 WHEN 'ja' THEN 5 WHEN 'de' THEN 6 WHEN 'ru' THEN 7 WHEN 'fr' THEN 8 WHEN 'ko' THEN 9 ELSE 0 END \
    AS locale_choice \
    FROM accounts a JOIN user_logins l ON l.account_id = a.id \
    WHERE a.id = ?1 AND l.suspended_at IS NULL AND l.authentication_epoch = ?3 \
    AND NOT EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id = a.id \
    AND r.target_epoch = l.authentication_epoch AND r.completed_at IS NULL)";

#[derive(sqlx::FromRow)]
struct LoginSnapshotRow {
    #[sqlx(flatten)]
    user: UserRow,
    suspended_at: Option<i64>,
    authentication_epoch: i64,
    pending: bool,
}

impl AccountStore {
    /// One lookup returns the PHC and original epoch together, without skipping the
    /// candidate for pending/suspension (verification/accounting ordering stays owned
    /// by the future application). Not used by current sign-in/session routes yet.
    pub async fn find_login_snapshot(&self, login: &str) -> Result<Option<CredentialLoginSnapshot>, StorageError> {
        let row: Option<LoginSnapshotRow> = sqlx::query_as(LOGIN_SNAPSHOT).bind(login).fetch_optional(&self.pool).await?;
        row.map(|row| {
            let epoch =
                CredentialEpoch::new(row.authentication_epoch).ok_or_else(|| StorageError::Corrupt("invalid credential epoch".into()))?;
            let (user, password_hash) = row.user.into_user_and_hash()?;
            Ok(CredentialLoginSnapshot {
                login: LoginRecord { user, password_hash, suspended: row.suspended_at.is_some() },
                fence: CredentialFence { epoch, pending: row.pending },
            })
        })
        .transpose()
    }

    /// Alternate session projection: original snapshot epoch, active account and current
    /// pending barrier are checked with both request flags in one SQL statement. Never
    /// replace `original` with the latest account epoch after authenticating old proof.
    /// Public trusted-User issuance remains a separate explicit compatibility authority.
    pub async fn find_session_user_at_epoch(
        &self,
        account: AccountId,
        with_notifications: bool,
        original: CredentialEpoch,
    ) -> Result<Option<SessionUser>, StorageError> {
        let row: Option<SessionUserRow> = sqlx::query_as(FENCED_SESSION_USER)
            .bind(account.0)
            .bind(with_notifications)
            .bind(original.get())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let (user, _) = row.user.into_user_and_hash()?;
            Ok(SessionUser {
                user,
                password_change_required: row.password_change_required,
                has_unread_notifications: row.has_unread_notifications,
                locale: super::super::preferences::locale_from_code(row.locale_choice),
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests;
