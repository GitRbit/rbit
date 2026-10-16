//! Organisation teams, their members and their repository permissions.
//!
//! Milestone 2 workstream A (access) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct TeamStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl TeamStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
