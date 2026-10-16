//! Password-reset and email-verification tokens (digests only).
//!
//! Milestone 1 workstream E owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct RecoveryStore {
    pool: SqlitePool,
}

impl RecoveryStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
