//! Logical identity only; binding does not enroll KV mutation workers yet.
use super::*;
use crate::Database;

#[tokio::test]
async fn kv_and_session_clones_retain_the_database_domain_before_any_service() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let clone = database.clone();
    let kv = database.open_kv(&dir.path().join("kv.redb")).unwrap();
    let retained = kv.sessions().clone();
    assert!(database.validate_kv_session_binding(&kv).is_ok());
    assert!(clone.validate_kv_session_binding(&kv.clone()).is_ok());
    assert!(retained.is_bound_to(&kv.writer));
    assert!(kv.sessions().is_bound_to(&kv.writer));
    assert!(!retained.is_bound_to(&rbit_authority_fence::Domain::new().writer()));
    database.close().await;
    // Identity survives terminal writer stop; this does not certify readiness.
    assert!(clone.validate_kv_session_binding(&kv).is_ok());
    assert!(retained.is_bound_to(&kv.writer));
}

#[tokio::test]
async fn separate_database_opens_even_on_the_same_file_cannot_silently_rebind_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit.db");
    let original = Database::open(&path).await.unwrap();
    let independent = Database::open(&path).await.unwrap();
    let kv = original.open_kv(&dir.path().join("kv.redb")).unwrap();
    assert!(matches!(independent.validate_kv_session_binding(&kv), Err(StorageError::Invariant("session authority domain mismatch"))));
    assert!(original.validate_kv_session_binding(&kv).is_ok());
    independent.close().await;
    original.close().await;
}
