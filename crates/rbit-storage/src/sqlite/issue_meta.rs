//! Labels, milestones, assignees, and the tables that de-duplicate commit and cross-references.
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct IssueMetaStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl IssueMetaStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
