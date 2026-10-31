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
            SignUp { username: "administrator".into(), email: "admin@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    assert!(!services.actions_available());
    assert!(services.actions.is_none());
    assert!(services.actions_status().await.is_none());
    assert_eq!(services.ui_snapshot().entries.len(), 0, "absent plugin code never registers UI");
    assert!(matches!(services.set_actions_enabled(&administrator, true).await, Err(rbit_services::ServiceError::Invalid(_))));
    assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), None);
    services.set_actions_enabled(&administrator, false).await.unwrap();
    assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), Some(false));
    assert_eq!(services.actions_background_tasks(), 0);
    services.shutdown().await;
}

#[tokio::test]
async fn persisted_intent_overrides_the_startup_default_in_core_only_builds() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().join("data");
    config.updates.check_for_updates = false;
    config.actions.enabled = true;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let settings = database.plugin_settings();

    settings.set_desired_enabled("rbit.actions", false, None, rbit_core::Timestamp(1)).await.unwrap();
    let services = Services::new(Arc::new(config.clone()), &database, &kv).await.unwrap();
    assert!(services.actions.is_none(), "explicit disablement overrides an enabled deployment default");
    services.shutdown().await;

    config.actions.enabled = false;
    settings.set_desired_enabled("rbit.actions", true, None, rbit_core::Timestamp(2)).await.unwrap();
    let result = Services::new(Arc::new(config), &database, &kv).await;
    assert!(matches!(result, Err(rbit_services::ServiceError::Invalid(_))), "persisted enablement must report missing Actions code");
    assert_eq!(settings.desired_enabled("rbit.actions").await.unwrap(), Some(true));
}
