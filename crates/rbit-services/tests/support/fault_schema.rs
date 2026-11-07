//! Install test-only DDL before any final application pool caches schema/QPSG state.
use rbit_storage::Database;
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;

pub(crate) async fn open(path: &Path, schema: Option<&str>) -> Database {
    let database = Database::open(path).await.unwrap();
    let Some(schema) = schema else {
        return database;
    };
    // Migrations and writer registration stay official; external DDL runs only
    // after every migration-pool connection closes, before final pool creation.
    database.close().await;
    let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new().filename(path));
    let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::raw_sql(schema).execute(&mut connection).await.unwrap();
    connection.close().await.unwrap();
    Database::open(path).await.unwrap()
}
