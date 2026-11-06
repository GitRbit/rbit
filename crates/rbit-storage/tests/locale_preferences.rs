//! Real preference migration, independent updates and the existing one-query session projection.
use rbit_identity_models::AccountLocale;
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{Database, NewUser, Work, count_work, sqlite::writer_compatibility::register};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[tokio::test]
async fn canonical_language_round_trips_and_browser_default_preserves_theme() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "fixture-only".into(),
            is_admin: false,
        })
        .await
        .unwrap();
    let store = database.preferences();
    assert_eq!(store.locale(user.id()).await.unwrap(), None);
    for locale in AccountLocale::ALL {
        store.set_locale(user.id(), Some(locale)).await.unwrap();
        store.set_theme(user.id(), "dark").await.unwrap();
        assert_eq!(store.locale(user.id()).await.unwrap(), Some(locale));
        let (session, work) = count_work(database.accounts().find_session_user(user.id(), false)).await;
        assert_eq!(session.unwrap().unwrap().locale, Some(locale));
        assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        for notifications in [false, true] {
            let (session, work) =
                count_work(database.accounts().find_session_user_at_epoch(user.id(), notifications, CredentialEpoch::INITIAL)).await;
            assert_eq!(session.unwrap().unwrap().locale, Some(locale));
            assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
        }
        let snapshot = database.accounts().find_login_snapshot("alice").await.unwrap().unwrap();
        assert_eq!(snapshot.fence.epoch, CredentialEpoch::INITIAL);
        assert_eq!(snapshot.login.password_hash, "fixture-only", "language writes do not alter credentials");
    }
    store.set_locale(user.id(), None).await.unwrap();
    assert_eq!(store.locale(user.id()).await.unwrap(), None);
    assert_eq!(store.theme(user.id()).await.unwrap().as_deref(), Some("dark"));
    assert_eq!(database.accounts().find_session_user(user.id(), false).await.unwrap().unwrap().locale, None);
    database.close().await;
}

#[tokio::test]
async fn upgrade_keeps_existing_theme_and_rejects_noncanonical_or_diagnostic_tags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit.db");
    let options = register(SqliteConnectOptions::new().filename(&path).create_if_missing(true).foreign_keys(true));
    let raw = SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap();
    sqlx::migrate!("./migrations").run_to(61, &raw).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','alice',1,1); \
        INSERT INTO user_logins(account_id,email,password_hash) VALUES(1,'alice@example.com','fixture-only'); \
        INSERT INTO user_preferences(account_id,theme,updated_at) VALUES(1,'dark',42);",
    )
    .execute(&raw)
    .await
    .unwrap();
    raw.close().await;
    let database = Database::open(&path).await.unwrap();
    let account = rbit_core::AccountId(1);
    assert_eq!(database.preferences().locale(account).await.unwrap(), None);
    assert_eq!(database.preferences().theme(account).await.unwrap().as_deref(), Some("dark"));
    let raw = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    let updated: i64 = sqlx::query_scalar("SELECT updated_at FROM user_preferences WHERE account_id=1").fetch_one(&raw).await.unwrap();
    assert_eq!(updated, 42);
    for invalid in ["", "EN", "es-MX", "en-XA", "ar-XB", "../es", "<script>", "en\nes"] {
        let error = sqlx::query("UPDATE user_logins SET locale=? WHERE account_id=1").bind(invalid).execute(&raw).await.unwrap_err();
        let database_error = error.as_database_error().expect("SQLite constraint error");
        assert!(database_error.message().contains("CHECK constraint failed"), "{database_error}");
        assert!(database_error.message().contains("locale"), "{database_error}");
    }
    assert_eq!(database.accounts().find_session_user(account, false).await.unwrap().unwrap().locale, None);
    raw.close().await;
    database.close().await;
}
