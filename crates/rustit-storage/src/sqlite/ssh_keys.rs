//! User SSH keys and repository deploy keys.
//!
//! Milestone 1 workstream H owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct SshKeyStore {
    pool: SqlitePool,
}

impl SshKeyStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
