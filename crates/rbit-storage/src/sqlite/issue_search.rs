//! Issue and pull request lists: filters, sorting, counts per tab and full-text search (FTS5).
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct IssueSearchStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl IssueSearchStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
