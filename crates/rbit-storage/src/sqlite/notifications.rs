//! In-app notifications: thread subscriptions and each user's inbox.
//!
//! Milestone 2 workstream I (issues) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct NotificationStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl NotificationStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
