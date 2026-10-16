//! Releases and the metadata of their attached files (the bytes live on disk).
//!
//! Milestone 2 workstream R (repository essentials) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ReleaseStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl ReleaseStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
