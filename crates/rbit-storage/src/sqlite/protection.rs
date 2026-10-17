//! Branch protection rules with their allowed users and teams.
//!
//! Milestone 2 workstream B (branch protection) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct BranchProtectionStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl BranchProtectionStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
