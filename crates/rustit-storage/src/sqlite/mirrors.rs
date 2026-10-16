//! Pull mirror settings and fetch status.
//!
//! Milestone 2 workstream R (repository essentials) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct MirrorStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl MirrorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
