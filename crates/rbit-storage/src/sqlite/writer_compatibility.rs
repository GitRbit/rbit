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
/// Migration0068's separate ACTIVE capability is also denied. Registering stopped
/// v2 never grants v3; its accepted-pair factory is currently test-gated and private.
/// Mode/version checks are compatibility controls, never DB-owner authentication.
/// Read-only tools
/// need not opt in. Do not use this to hide a mixed-version deployment.
/// Wiki registration is LEGACY-only: managed Wiki authority belongs exclusively
/// to the private owned-FULL/root-guard composition, not this general connection.
#[must_use]
pub fn register(options: SqliteConnectOptions) -> SqliteConnectOptions {
    super::wiki_mutations::protocol::legacy_options(options)
        .collation("rbit_credential_writer_v1", str::cmp)
        .collation("rbit_credential_layout_v2", str::cmp)
        .collation("rbit_credential_fenced_v2", str::cmp)
        .collation("rbit_credential_active_v3", str::cmp)
}

pub(crate) fn database_options(path: &Path) -> Result<SqliteConnectOptions, sqlx::Error> {
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
    register(options).collation("rbit_credential_fenced_v2", fenced_comparison)
}

/// Capability ordering is natural ordering of deterministic canonical keys. Exactly
/// the two reserved guard tokens become equivalent; ALL inputs remain reflexive,
/// symmetric and transitive as required by SQLite's collation callback contract.
fn equivalent_comparison(a: &str, b: &str, requested: &str, accepted: &str) -> std::cmp::Ordering {
    let a = if a == accepted { requested } else { a };
    let b = if b == accepted { requested } else { b };
    a.cmp(b)
}
pub(crate) fn fenced_comparison(a: &str, b: &str) -> std::cmp::Ordering {
    equivalent_comparison(a, b, "rbit-fenced-v2/requested", "rbit-fenced-v2/accepted")
}
#[cfg(test)]
pub(crate) fn active_comparison(a: &str, b: &str) -> std::cmp::Ordering {
    equivalent_comparison(a, b, "rbit-active-v3/requested", "rbit-active-v3/accepted")
}
#[cfg(test)]
mod tests;
