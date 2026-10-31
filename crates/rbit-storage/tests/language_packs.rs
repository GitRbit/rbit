//! Installation limits, atomic replacements, account preferences and SQLite persistence.

use rbit_core::Timestamp;
use rbit_storage::{
    Database, NewUser,
    sqlite::language_packs::{MAX_LANGUAGE_PACK_BYTES, MAX_LANGUAGE_PACKS, NewLanguagePack},
};

fn pack<'a>(id: &'a str, bytes: &'a [u8]) -> NewLanguagePack<'a> {
    NewLanguagePack {
        id,
        locale: "fr-FR",
        version: "1.0.0",
        bytes,
        digest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        message_count: 1,
        message_total: 10,
        installed_by: None,
        updated_at: Timestamp(123),
    }
}

#[tokio::test]
async fn archive_bytes_and_metadata_survive_database_reopen_and_replace_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rbit.db");
    let database = Database::open(&path).await.unwrap();
    let store = database.language_packs();
    assert!(store.put(&pack("example.french", &[1, 2, 3])).await.unwrap());
    assert!(store.put(&NewLanguagePack { version: "2.0.0", updated_at: Timestamp(456), ..pack("example.french", &[4, 5]) }).await.unwrap());
    drop(store);
    drop(database);
    let reopened = Database::open(&path).await.unwrap();
    let rows = reopened.language_packs().list().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((&rows[0].id, &rows[0].locale, &rows[0].version), (&"example.french".to_owned(), &"fr-FR".to_owned(), &"2.0.0".to_owned()));
    assert_eq!(rows[0].bytes, [4, 5]);
    assert_eq!(rows[0].updated_at, Timestamp(456));
    assert!(reopened.language_packs().remove("example.french").await.unwrap());
    assert!(!reopened.language_packs().remove("example.french").await.unwrap());
}

#[tokio::test]
async fn concurrent_installations_cannot_exceed_capacity_but_updates_still_work() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.language_packs();
    for index in 0..MAX_LANGUAGE_PACKS - 1 {
        assert!(store.put(&pack(&format!("example.pack-{index}"), &[1])).await.unwrap());
    }
    let first_pack = pack("example.first", &[1]);
    let second_pack = pack("example.second", &[2]);
    let (first, second) = tokio::join!(store.put(&first_pack), store.put(&second_pack));
    assert_ne!(first.unwrap(), second.unwrap(), "only one concurrent writer may claim the final slot");
    assert_eq!(store.list().await.unwrap().len(), MAX_LANGUAGE_PACKS);
    assert!(store.put(&pack("example.pack-0", &[3])).await.unwrap());
    assert_eq!(store.get("example.pack-0").await.unwrap().unwrap().bytes, [3]);
    assert!(!store.put(&pack("example.overflow", &[1])).await.unwrap());
}

#[tokio::test]
async fn schema_rejects_oversized_archives_and_invalid_metadata_without_changing_existing_rows() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let store = database.language_packs();
    assert!(store.put(&pack("example.french", &[1])).await.unwrap());
    let oversized = vec![0; MAX_LANGUAGE_PACK_BYTES + 1];
    assert!(store.put(&pack("example.french", &oversized)).await.is_err());
    assert!(store.put(&pack("../outside", &[1])).await.is_err());
    assert!(store.put(&NewLanguagePack { locale: "fr\r\nX-Forged: yes", ..pack("example.invalid", &[1]) }).await.is_err());
    assert!(store.put(&NewLanguagePack { digest: "not-a-digest", ..pack("example.invalid", &[1]) }).await.is_err());
    assert_eq!(store.get("example.french").await.unwrap().unwrap().bytes, [1]);
    assert_eq!(store.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn preference_is_in_the_existing_session_lookup_and_survives_package_removal() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(&directory.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    let store = database.language_packs();
    assert_eq!(database.accounts().find_session_user(user.id(), false).await.unwrap().unwrap().locale, None);
    store.set_account_locale(user.id(), Some("fr-FR")).await.unwrap();
    let queries = crate::query_count::QueryCounter::new();
    let resumed = queries.measure(database.accounts().find_session_user(user.id(), false)).await.unwrap().unwrap();
    assert_eq!(resumed.locale.as_deref(), Some("fr-FR"));
    assert_eq!(queries.queries().len(), 1, "locale preference shares the existing session lookup");
    store.put(&pack("example.french", &[1])).await.unwrap();
    store.remove("example.french").await.unwrap();
    assert_eq!(store.account_locale(user.id()).await.unwrap().as_deref(), Some("fr-FR"));
    store.set_account_locale(user.id(), None).await.unwrap();
    assert_eq!(database.accounts().find_session_user(user.id(), true).await.unwrap().unwrap().locale, None);
}

#[tokio::test]
async fn sqlite_backup_contains_archives_without_an_installed_provider_or_filesystem_copy_hook() {
    use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rbit.db");
    let snapshot = directory.path().join("backup.db");
    let database = Database::open(&path).await.unwrap();
    let store = database.language_packs();
    store.put(&pack("example.french", &[1, 2, 3])).await.unwrap();
    let mut connection = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await.unwrap();
    sqlx::query("VACUUM INTO ?").bind(snapshot.to_str().unwrap()).execute(&mut connection).await.unwrap();
    store.put(&NewLanguagePack { version: "2.0.0", ..pack("example.french", &[4, 5]) }).await.unwrap();

    let backup = Database::open(&snapshot).await.unwrap();
    let retained = backup.language_packs().get("example.french").await.unwrap().unwrap();
    assert_eq!(retained.bytes, [1, 2, 3]);
    assert_eq!(retained.version, "1.0.0");
    assert_eq!(database.language_packs().get("example.french").await.unwrap().unwrap().bytes, [4, 5]);
}
