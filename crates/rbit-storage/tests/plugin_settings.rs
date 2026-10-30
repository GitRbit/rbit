//! Plugin intent survives restarts and cannot erase incomplete credential revocation.

use rbit_core::Timestamp;
use rbit_storage::Database;

#[tokio::test]
async fn intent_and_pending_revocation_survive_database_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rbit.db");
    let database = Database::open(&path).await.unwrap();
    let settings = database.plugin_settings();
    assert_eq!(settings.desired_enabled("rbit.actions").await.unwrap(), None);
    assert!(!settings.disable_pending("rbit.actions").await.unwrap());
    settings.set_desired_enabled("rbit.actions", false, None, Timestamp(1)).await.unwrap();
    settings.set_desired_enabled("rbit.actions", true, None, Timestamp(2)).await.unwrap();
    assert!(settings.disable_pending("rbit.actions").await.unwrap());
    drop(settings);
    drop(database);
    let reopened = Database::open(&path).await.unwrap();
    let settings = reopened.plugin_settings();
    assert_eq!(settings.desired_enabled("rbit.actions").await.unwrap(), Some(true));
    assert!(settings.disable_pending("rbit.actions").await.unwrap());
    settings.complete_disable("rbit.actions").await.unwrap();
    assert!(!settings.disable_pending("rbit.actions").await.unwrap());
    settings.set_desired_enabled("rbit.actions", false, None, Timestamp(3)).await.unwrap();
    assert!(settings.disable_pending("rbit.actions").await.unwrap());
}

#[tokio::test]
async fn plugin_intent_does_not_change_other_plugin_rows() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let settings = database.plugin_settings();
    settings.set_desired_enabled("rbit.other", true, None, Timestamp(1)).await.unwrap();
    settings.set_desired_enabled("rbit.actions", false, None, Timestamp(2)).await.unwrap();
    assert_eq!(settings.desired_enabled("rbit.other").await.unwrap(), Some(true));
    assert!(!settings.disable_pending("rbit.other").await.unwrap());
}
