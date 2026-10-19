//! Preference use-cases against real, temporary `SQLite` and redb stores.

use std::sync::Arc;

use rbit_core::{Config, User};
use rbit_services::{ServiceError, Services};
use rbit_storage::{Database, KvStore, NewUser};

async fn fixture() -> (tempfile::TempDir, Database, Services) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
    let services = Services::new(Arc::new(Config::default()), &database, &kv).await.unwrap();
    (dir, database, services)
}

async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn theme_preference_is_per_user() {
    let (_dir, database, services) = fixture().await;
    let alice = user(&database, "alice").await;
    let bob = user(&database, "bob").await;
    let preferences = &services.preferences;

    assert_eq!(preferences.theme(&alice).await.unwrap(), None);
    preferences.set_theme(&alice, "base24-dracula").await.unwrap();
    preferences.set_theme(&bob, "light").await.unwrap();
    assert_eq!(preferences.theme(&alice).await.unwrap().as_deref(), Some("base24-dracula"));
    assert_eq!(preferences.theme(&bob).await.unwrap().as_deref(), Some("light"));

    preferences.set_theme(&alice, "system").await.unwrap();
    assert_eq!(preferences.theme(&alice).await.unwrap().as_deref(), Some("system"));
}

#[tokio::test]
async fn malformed_theme_slugs_are_rejected() {
    let (_dir, database, services) = fixture().await;
    let alice = user(&database, "alice").await;
    for bad in ["", "Dark", "a b", "../../etc", "<script>"] {
        let result = services.preferences.set_theme(&alice, bad).await;
        assert!(matches!(result, Err(ServiceError::Invalid(_))), "{bad:?} must be rejected");
    }
    assert_eq!(services.preferences.theme(&alice).await.unwrap(), None);
}
