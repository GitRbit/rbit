//! Core-only builds preserve English and locale preferences without a catalog engine.

use rbit_core::{Config, Timestamp};
use rbit_services::{ServiceError, Services};
use rbit_storage::{Database, KvStore, NewUser};
use std::sync::Arc;

#[tokio::test]
async fn unavailable_provider_rejects_enablement_without_losing_preferences_or_intent() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().join("data");
    config.updates.check_for_updates = false;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let admin = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: true,
        })
        .await
        .unwrap();
    config.i18n.enabled = true;
    database.plugin_settings().set_desired_enabled("rbit.i18n", false, Some(admin.id()), Timestamp::now()).await.unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    assert!(!services.i18n_available());
    assert!(!services.i18n_enabled());
    assert_eq!(services.locale_snapshot(Some("fr"), None, None).lang(), "en");
    assert!(matches!(services.set_i18n_enabled(&admin, true).await, Err(ServiceError::Invalid(_))));
    assert!(
        matches!(services.install_language_pack(&admin, b"archive").await, Err(ServiceError::Invalid(message)) if message.contains("without i18n"))
    );
    assert_eq!(services.language_packs(&admin).await.unwrap(), Vec::new());
    assert_eq!(database.plugin_settings().desired_enabled("rbit.i18n").await.unwrap(), Some(false));
    services.set_account_locale(&admin, Some("fr")).await.unwrap();
    assert_eq!(database.accounts().find_session_user(admin.id(), false).await.unwrap().unwrap().locale.as_deref(), Some("fr"));
    services.shutdown().await;
}

#[tokio::test]
async fn an_effectively_enabled_provider_missing_from_the_distribution_is_a_startup_error() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().join("data");
    config.updates.check_for_updates = false;
    config.i18n.enabled = true;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    assert!(
        matches!(Services::new(Arc::new(config), &database, &kv).await, Err(ServiceError::Invalid(message)) if message.contains("without i18n"))
    );
    assert_eq!(database.plugin_settings().desired_enabled("rbit.i18n").await.unwrap(), None);
}
