//! Repository collaborators and the permission each one has.
//!
//! Milestone 2 workstream A (access) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct CollaboratorStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl CollaboratorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
