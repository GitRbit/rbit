//! Install injected SQL failures before opening the pool used by the application.
use super::{Arc, Config, Fixture, KvStore, Services};

impl Fixture {
    pub(super) async fn with_fault_schema(schema: &'static str) -> Self {
        Self::configured(Some(schema)).await
    }

    pub(super) async fn configured(schema: Option<&'static str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rbit.db");
        let database = crate::fault_schema::open(&path, schema).await;
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { dir, database, services }
    }
}
