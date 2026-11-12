//! Request ids of agent calls (migration `0080_agent_requests.sql`): what a call that named
//! one answered, so its retry gets the same answer (agent plan step 4.3).
use rbit_core::{AccountId, Timestamp};
use sqlx::SqlitePool;

use crate::StorageError;

/// How long a request id is remembered.
pub const REQUEST_ID_SECONDS: i64 = 7 * 24 * 60 * 60;

/// A remembered request: the fingerprint of its inputs and what it answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    pub fingerprint: String,
    pub answer: String,
}

#[derive(Debug, Clone)]
pub struct AgentRequestStore {
    pool: SqlitePool,
}

impl AgentRequestStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// What `account`'s request `id` answered, if it is remembered and younger than a week.
    pub async fn find(&self, account: AccountId, id: &str, now: Timestamp) -> Result<Option<Remembered>, StorageError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT fingerprint, answer FROM agent_requests WHERE account_id = ? AND request_id = ? AND created_at >= ?",
        )
        .bind(account.0)
        .bind(id)
        .bind(now.0 - REQUEST_ID_SECONDS)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(fingerprint, answer)| Remembered { fingerprint, answer }))
    }

    /// Remembers that `account`'s request `id` with inputs `fingerprint` answered `answer`,
    /// forgetting requests older than a week. A request already remembered keeps its answer.
    pub async fn remember(&self, account: AccountId, id: &str, fingerprint: &str, answer: &str, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM agent_requests WHERE created_at < ?").bind(now.0 - REQUEST_ID_SECONDS).execute(&self.pool).await?;
        sqlx::query(
            "INSERT INTO agent_requests (account_id, request_id, fingerprint, answer, created_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (account_id, request_id) DO UPDATE SET fingerprint = excluded.fingerprint, answer = excluded.answer, \
             created_at = excluded.created_at WHERE agent_requests.created_at < ?",
        )
        .bind(account.0)
        .bind(id)
        .bind(fingerprint)
        .bind(answer)
        .bind(now.0)
        .bind(now.0 - REQUEST_ID_SECONDS)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
