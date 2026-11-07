//! Administrator saves validate password capacity before persistence; changes need restart.

use std::sync::Arc;

use rbit_core::Config;
use rbit_services::{ServiceError, Services};
use rbit_storage::{Database, KvStore, NewUser};

#[tokio::test]
async fn admin_save_rejects_invalid_capacity_atomically_and_defaults_older_documents() {
    let directory = tempfile::tempdir().unwrap();
    let mut configured = Config::default();
    configured.storage.data_dir = directory.path().to_path_buf();
    let config = Arc::new(configured);
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(config.clone(), &database, &kv).await.unwrap();
    let admin = database
        .accounts()
        .create_user(NewUser {
            name: "root".parse().unwrap(),
            email: "root@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused-test-hash".into(),
            is_admin: true,
        })
        .await
        .unwrap();
    let mut document: serde_json::Value = serde_json::from_str(&services.admin.configuration(&admin).await.unwrap()).unwrap();
    document["security"]["max_password_jobs"] = 4.into();
    services.admin.stage_configuration(&admin, &document.to_string()).await.unwrap();
    let saved = services.admin.configuration(&admin).await.unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&saved).unwrap()["security"]["max_password_jobs"], 4);
    assert_eq!(config.security.max_password_jobs, 2, "staged capacity must not mutate the running configuration");
    for invalid in [0, 65] {
        document["security"]["max_password_jobs"] = invalid.into();
        let error = services.admin.stage_configuration(&admin, &document.to_string()).await.unwrap_err();
        assert!(matches!(error, ServiceError::Invalid(message) if message.contains("security.max_password_jobs")));
        assert_eq!(services.admin.configuration(&admin).await.unwrap(), saved);
    }
    document["security"].as_object_mut().unwrap().remove("max_password_jobs");
    services.admin.stage_configuration(&admin, &document.to_string()).await.unwrap();
    let old_document = services.admin.configuration(&admin).await.unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&old_document).unwrap()["security"]["max_password_jobs"], 2);
}
