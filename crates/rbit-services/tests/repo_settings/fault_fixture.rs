//! Install injected SQL failures before opening the pool used by the application.
use super::{Arc, Config, Database, Fixture, KvStore, Services};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

impl Fixture {
    pub(super) async fn with_fault_schema(schema: &'static str) -> Self {
        Self::configured(Some(schema)).await
    }

    pub(super) async fn configured(schema: Option<&'static str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rbit.db");
        let mut database = Database::open(&path).await.unwrap();
        if let Some(schema) = schema {
            database.close().await;
            let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new().filename(&path));
            let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
            sqlx::raw_sql(schema).execute(&mut connection).await.unwrap();
            connection.close().await.unwrap();
            database = Database::open(&path).await.unwrap();
        }
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { dir, database, services }
    }
}
