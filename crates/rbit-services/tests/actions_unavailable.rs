//! A core-only distribution remains usable and cannot silently enable absent code.

use std::sync::Arc;

use rbit_core::Config;
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};

#[tokio::test]
async fn absent_actions_cannot_be_enabled_and_disabling_persists_intent() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().join("data");
    config.updates.check_for_updates = false;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let administrator = services
        .accounts
        .create_user(
            SignUp { username: "admin".into(), email: "admin@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    assert!(!services.actions_available());
    assert!(services.actions.is_none());
    assert!(services.actions_status().await.is_none());
    assert!(matches!(services.set_actions_enabled(&administrator, true).await, Err(rbit_services::ServiceError::Invalid(_))));
    assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), None);
    services.set_actions_enabled(&administrator, false).await.unwrap();
    assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), Some(false));
    assert_eq!(services.actions_background_tasks(), 0);
    services.shutdown().await;
}
