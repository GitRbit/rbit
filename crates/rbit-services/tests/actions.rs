//! The Actions service exists only when `[actions] enabled` is set, and starts no
//! background work or files of its own otherwise.

use std::sync::Arc;

use rbit_core::Config;
use rbit_services::Services;
use rbit_storage::{Database, KvStore};

async fn services(configure: impl FnOnce(&mut Config)) -> (tempfile::TempDir, Arc<Config>, Services) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("data");
    configure(&mut config);
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let config = Arc::new(config);
    let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
    (dir, config, services)
}

#[tokio::test]
async fn disabled_actions_have_no_service_and_no_background_work() {
    let (_dir, config, services) = services(|_| {}).await;
    assert!(!config.actions.enabled);
    assert!(services.actions.is_none());
    services.spawn_background();
    assert_eq!(services.actions_background_tasks(), 0);
    services.shutdown().await;
    assert!(!config.storage.actions_dir().exists());
}

#[tokio::test]
async fn enabled_actions_have_a_service_with_the_configured_limits() {
    let (_dir, _config, services) = services(|config| {
        config.actions.enabled = true;
        config.actions.max_workflow_bytes = 4096;
        config.actions.evaluation_concurrency = 3;
    })
    .await;
    let actions = services.actions.as_ref().expect("the service exists when enabled");
    assert_eq!(actions.limits().max_bytes, 4096);
    assert_eq!(actions.evaluations().available_permits(), 3);
    let version = actions.tasks_version();
    assert!(version > 0);
    actions.bump_tasks_version();
    assert_eq!(actions.tasks_version(), version + 1);
    services.spawn_background();
    services.spawn_background();
    services.shutdown().await;
}

#[tokio::test]
async fn shutdown_before_start_prevents_background_work_and_secret_key_creation() {
    let (_dir, config, services) = services(|config| config.actions.enabled = true).await;
    let actions = services.actions.as_ref().unwrap();
    actions.stop_background().await.unwrap();
    actions.spawn_background();
    tokio::task::yield_now().await;
    assert_eq!(actions.background_tasks(), 0);
    assert!(!config.storage.actions_dir().exists());
    actions.stop_background().await.unwrap();
}

#[tokio::test]
async fn shutdown_disposes_started_tasks_and_cannot_restart_the_same_scope() {
    let (_dir, _config, services) = services(|config| config.actions.enabled = true).await;
    let actions = services.actions.as_ref().unwrap();
    actions.spawn_background();
    actions.stop_background().await.unwrap();
    assert_eq!(actions.background_tasks(), 0);
    actions.spawn_background();
    tokio::task::yield_now().await;
    assert_eq!(actions.background_tasks(), 0);
}
