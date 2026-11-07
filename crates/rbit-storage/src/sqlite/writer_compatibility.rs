//! Connection-local software compatibility for protected credential/account writes.
//!
//! Migration 0061 uses this collation in persistent BEFORE-DML triggers. A missing
//! collation rejects even an already-open older connection when SQLite reparses
//! its cached statement after the schema changes. This is not DB-owner security.
use std::{path::Path, str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};

/// Registers the exact writer protocol required by migration 0061.
///
/// Use for intentional direct SQL writers as well as pooled connections. This pure
/// comparison has no credentials, state, IO or session-fencing authority. Registering
/// a different protocol does not satisfy the persisted v1 triggers. Migration0067
/// also requires the compatible v2 layout marker. This ordinary registration DENIES
/// fenced writes; only the private stopped owner receives that separate capability.
/// Mode/version checks are compatibility controls, never DB-owner authentication.
/// Read-only tools
/// need not opt in. Do not use this to hide a mixed-version deployment.
#[must_use]
pub fn register(options: SqliteConnectOptions) -> SqliteConnectOptions {
    options
        .collation("rbit_credential_writer_v1", str::cmp)
        .collation("rbit_credential_layout_v2", str::cmp)
        .collation("rbit_credential_fenced_v2", |_, _| std::cmp::Ordering::Greater)
}

pub(super) fn database_options(path: &Path) -> Result<SqliteConnectOptions, sqlx::Error> {
    Ok(register(
        SqliteConnectOptions::from_str("sqlite:")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "memory"),
    ))
}

/// Private stopped-upgrade connection, never installed on ordinary application pools.
pub(crate) fn register_stopped(options: SqliteConnectOptions) -> SqliteConnectOptions {
    register(options).collation("rbit_credential_fenced_v2", str::cmp)
}
