//! Pull request reviews, review comments on diff lines and review requests.
//!
//! Milestone 2 workstream P (pull requests) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ReviewStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl ReviewStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
