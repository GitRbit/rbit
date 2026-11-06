//! Direct, registered fixture connections; separate from the official tuned pool.
use std::{path::Path, str::FromStr};

use rbit_storage::sqlite::writer_compatibility::register;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

pub(super) async fn pool(path: &Path) -> SqlitePool {
    let options = register(SqliteConnectOptions::from_str("sqlite:").unwrap()).filename(path).create_if_missing(true).foreign_keys(true);
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}
