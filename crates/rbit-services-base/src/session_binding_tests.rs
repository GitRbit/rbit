//! Mismatched logical bindings must fail before the service's first query/bootstrap.
use super::*;

#[tokio::test]
async fn mismatched_session_binding_is_rejected_before_sql_or_bootstrap_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let original = Database::open(&dir.path().join("original.db")).await.unwrap();
    let other = Database::open(&dir.path().join("other.db")).await.unwrap();
    let kv = other.open_kv(&dir.path().join("kv.redb")).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("not-initialized");
    let untouched = config.storage.data_dir.clone();
    original.close().await;
    // The closed pool would reject the original first SQL query. Getting the
    // exact mismatch instead proves validation precedes that query; no claim
    // is made about constructor work that happened before this service call.
    let result = Box::pin(Services::new(Arc::new(config), &original, &kv)).await;
    assert!(matches!(result, Err(ServiceError::Conflict("session authority domain mismatch"))));
    assert!(!untouched.exists());
    other.close().await;
}
