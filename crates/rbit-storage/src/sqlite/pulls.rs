//! Pull requests: the branch data attached to an issue row, mergeability and merge results.
//!
//! Milestone 2 workstream P (pull requests) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct PullStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl PullStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
