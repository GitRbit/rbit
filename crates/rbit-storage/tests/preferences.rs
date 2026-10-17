//! Preference persistence against a real, migrated SQLite database.

use rbit_storage::{Database, NewUser};

#[tokio::test]
async fn theme_round_trips_and_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    let store = database.preferences();

    assert_eq!(store.theme(user.id()).await.unwrap(), None);
    store.set_theme(user.id(), "dark").await.unwrap();
    assert_eq!(store.theme(user.id()).await.unwrap().as_deref(), Some("dark"));
    store.set_theme(user.id(), "base16-gruvbox-dark-hard").await.unwrap();
    assert_eq!(store.theme(user.id()).await.unwrap().as_deref(), Some("base16-gruvbox-dark-hard"));

    // The schema bounds the value, whatever the caller validated.
    assert!(store.set_theme(user.id(), "").await.is_err());
    assert!(store.set_theme(user.id(), &"x".repeat(65)).await.is_err());
}
