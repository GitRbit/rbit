//! Explicit dormant SQL/KV identity binding; not a startup-mode or rollback proof.
use super::{CredentialRevocationStore, rows};
use crate::{EpochSessionStore, StorageError};
use rbit_identity_models::sessions::SessionPairIdentity;
const PAIR: &str = "SELECT CASE WHEN typeof(identity)='blob' THEN CASE WHEN length(identity)=16 THEN identity END END \
    FROM credential_session_pair WHERE singleton=1 \
    AND (SELECT count(*) FROM (SELECT 1 FROM credential_session_pair LIMIT 2))=1";
impl CredentialRevocationStore {
    /// Explicit composition only, after a separately accepted empty-KV initialization.
    /// No replacement/reset/adoption. Never called by migration or `Services::new`.
    pub async fn initialize_pair(&self, sessions: &EpochSessionStore) -> Result<SessionPairIdentity, StorageError> {
        #[cfg(test)]
        self.pool.require_legacy()?;
        let identity = sessions.pair_identity().await?.ok_or_else(rows::invalid)?;
        let bytes = identity.storage_bytes();
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT OR IGNORE INTO credential_session_pair(singleton,identity) VALUES(1,?)")
            .bind(bytes.as_slice())
            .execute(&mut *tx)
            .await?;
        let stored: Option<Option<Vec<u8>>> = sqlx::query_scalar(PAIR).fetch_optional(&mut *tx).await?;
        if stored.flatten().and_then(|bytes| SessionPairIdentity::from_storage(&bytes)) != Some(identity) {
            return Err(rows::invalid());
        }
        tx.commit().await?;
        Ok(identity)
    }
    /// Verify BEFORE any redb effect. The actual apply transaction checks this same
    /// immutable ID again; SQL acknowledgment also compares it atomically.
    pub async fn verify_pair(&self, sessions: &EpochSessionStore) -> Result<SessionPairIdentity, StorageError> {
        let identity = sessions.pair_identity().await?.ok_or_else(rows::invalid)?;
        let stored: Option<Option<Vec<u8>>> = sqlx::query_scalar(PAIR).fetch_optional(&self.pool).await?;
        if stored.flatten().and_then(|bytes| SessionPairIdentity::from_storage(&bytes)) != Some(identity) {
            return Err(rows::invalid());
        }
        Ok(identity)
    }
}
