//! Webhooks and their delivery log.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct WebhookStore {
    #[expect(dead_code, reason = "stub until the workstream implements the store")]
    pool: SqlitePool,
}

impl WebhookStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
