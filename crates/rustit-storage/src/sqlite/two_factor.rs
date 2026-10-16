//! TOTP secrets and hashed recovery codes.
//!
//! Milestone 1 workstream S owns this file. Keep row types here rather than in `rows.rs`.

use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct TwoFactorStore {
    pool: SqlitePool,
}

impl TwoFactorStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
