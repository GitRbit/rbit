//! Same-transaction globally allocated creation facts; no legacy mode selection.
use super::{AccountStore, NewUser, insert_user_original};
use crate::StorageError;
use rbit_identity_models::accounts::CreatedAccountSnapshot;
impl AccountStore {
    /// Original signup facts are published only after the authoritative transaction commits.
    pub async fn register_user_original(&self, new: NewUser, registration_open: bool) -> Result<CreatedAccountSnapshot, StorageError> {
        let mut tx = self.pool.begin().await?;
        let created = insert_user_original(&mut tx, new, Some(registration_open), false).await?;
        tx.commit().await?;
        Ok(created)
    }
    /// Explicit administrator/offline creation, without the first-user override.
    pub async fn create_user_original(&self, new: NewUser) -> Result<CreatedAccountSnapshot, StorageError> {
        let mut tx = self.pool.begin().await?;
        let created = insert_user_original(&mut tx, new, None, false).await?;
        tx.commit().await?;
        Ok(created)
    }
    /// Preserve temporary-password marking inside the creation transaction.
    pub async fn create_user_with_initial_password_original(&self, new: NewUser) -> Result<CreatedAccountSnapshot, StorageError> {
        let mut tx = self.pool.begin().await?;
        let created = insert_user_original(&mut tx, new, None, false).await?;
        sqlx::query("UPDATE user_logins SET password_change_required = 1 WHERE account_id = ?")
            .bind(created.user.id().0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(created)
    }
}
