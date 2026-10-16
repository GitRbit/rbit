//! Append-only security audit log.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AuditStore {
    pool: SqlitePool,
}

impl AuditStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
