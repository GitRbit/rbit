//! Real SQLite writer-version boundary; no credential/session-fencing activation.
use std::path::Path;

use rbit_storage::{Database, sqlite::writer_compatibility::register};
use sqlx::{Connection, SqliteConnection, SqlitePool, sqlite::SqliteConnectOptions};

const PROTECTED: &[&str] = &[
    "accounts",
    "user_logins",
    "credential_revocations",
    "two_factor",
    "recovery_codes",
    "passkeys",
    "external_identities",
    "email_tokens",
    "access_tokens",
    "ssh_keys",
    "deploy_keys",
    "server_secrets",
    "instance_settings",
];
const CHANGE: &str = "UPDATE user_logins SET password_hash=? WHERE account_id=1";

fn options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new().filename(path).create_if_missing(true).foreign_keys(true)
}
async fn pool(path: &Path) -> SqlitePool {
    SqlitePool::connect_with(register(options(path))).await.unwrap()
}
async fn legacy_fixture(path: &Path) -> SqlitePool {
    let pool = pool(path).await;
    sqlx::migrate!("./migrations").run_to(60, &pool).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','alice',1,1); \
         INSERT INTO user_logins(account_id,email,password_hash) VALUES(1,'alice@example.com','original');",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}
fn missing_protocol(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|error| error.message().contains("no such collation sequence: rbit_credential_writer_v1"))
}

#[tokio::test]
async fn cached_legacy_cli_writes_refuse_after_upgrade_but_reads_and_new_writers_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("writers.db");
    let pool = legacy_fixture(&path).await;
    let mut old = SqliteConnection::connect_with(&options(&path)).await.unwrap();
    sqlx::query(CHANGE).bind("before upgrade").execute(&mut old).await.unwrap();
    assert!(old.cached_statements_size() > 0, "exercise an already cached statement");
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let mut older = sqlx::migrate!("./migrations");
    older.migrations.to_mut().retain(|migration| migration.version <= 60);
    assert!(matches!(older.run(&pool).await, Err(sqlx::migrate::MigrateError::VersionMissing(61))));
    let error = sqlx::query(CHANGE).bind("stale writer").execute(&mut old).await.unwrap_err();
    assert!(missing_protocol(&error), "{error}");
    for statement in [
        "INSERT INTO accounts(kind,name,created_at,updated_at) VALUES('user','legacy',1,1)",
        "DELETE FROM accounts WHERE id=1",
        "UPDATE user_logins SET suspended_at=2 WHERE account_id=1",
    ] {
        let error = sqlx::query(statement).execute(&mut old).await.unwrap_err();
        assert!(missing_protocol(&error), "{error}");
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM user_logins WHERE account_id=1").fetch_one(&mut old).await.unwrap();
    assert_eq!(hash, "before upgrade");
    old.close().await.unwrap();
    pool.close().await;

    // Every newly established official connection receives the protocol, including
    // those established after the migration. Safe collations work with trusted_schema OFF.
    let mut new = SqliteConnection::connect_with(&register(options(&path)).pragma("trusted_schema", "OFF")).await.unwrap();
    sqlx::query(CHANGE).bind("new writer").execute(&mut new).await.unwrap();
    new.close().await.unwrap();
    let database = Database::open(&path).await.unwrap();
    assert_eq!(database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, "new writer");
    database.profiles().set_password_hash(rbit_core::AccountId(1), "official replacement").await.unwrap();
    let created = database
        .accounts()
        .create_user(rbit_storage::NewUser {
            name: "new-writer".parse().unwrap(),
            email: "new-writer@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "creation fixture".into(),
            is_admin: false,
        })
        .await
        .unwrap();
    assert_eq!(database.admin().set_suspended(created.id(), true).await.unwrap(), rbit_storage::sqlite::admin::Moderation::Applied);
    assert_eq!(database.admin().delete_user(created.id()).await.unwrap(), rbit_storage::sqlite::admin::Moderation::Applied);
    database.close().await;
}

#[tokio::test]
async fn every_scoped_dml_requires_the_exact_protocol_not_a_different_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scope.db");
    let pool = legacy_fixture(&path).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let mut unknown = SqliteConnection::connect_with(&options(&path).collation("rbit_credential_writer_v2", str::cmp)).await.unwrap();
    for table in PROTECTED {
        // Only this fixed internal allowlist supplies SQL identifiers, never user input.
        let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})"))).fetch_all(&pool).await.unwrap();
        let column = &columns[0].1;
        for statement in [
            format!("INSERT INTO {table} DEFAULT VALUES"),
            format!("UPDATE {table} SET {column}={column} WHERE 0"),
            format!("DELETE FROM {table} WHERE 0"),
        ] {
            let error = sqlx::query(sqlx::AssertSqlSafe(statement)).execute(&mut unknown).await.unwrap_err();
            assert!(missing_protocol(&error), "{table}: {error}");
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name LIKE 'credential_writer_%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, i64::try_from(PROTECTED.len() * 3).unwrap());
    // Unrelated data remains outside this compatibility contract.
    sqlx::query("INSERT INTO audit_events(category,action,created_at) VALUES('auth','fixture',1)").execute(&mut unknown).await.unwrap();
    unknown.close().await.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn migration_and_credential_updates_roll_back_without_partial_protocol_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollback.db");
    let pool = legacy_fixture(&path).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0061_credential_writer_compatibility.sql")).execute(&mut *tx).await.unwrap();
    sqlx::query(CHANGE).bind("rolled back").execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name LIKE 'credential_writer_%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let mut old = SqliteConnection::connect_with(&options(&path)).await.unwrap();
    sqlx::query(CHANGE).bind("before committed migration").execute(&mut old).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    assert!(missing_protocol(&sqlx::query(CHANGE).bind("refused").execute(&mut old).await.unwrap_err()));
    old.close().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(CHANGE).bind("not committed").execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM user_logins WHERE account_id=1").fetch_one(&pool).await.unwrap();
    assert_eq!(hash, "before committed migration");
    pool.close().await;
}

#[tokio::test]
async fn failed_migration_does_not_leave_a_partially_installed_writer_barrier() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("failed-migration.db");
    let pool = legacy_fixture(&path).await;
    sqlx::query("CREATE TRIGGER credential_writer_accounts_update BEFORE UPDATE ON accounts BEGIN SELECT 1; END")
        .execute(&pool)
        .await
        .unwrap();
    assert!(sqlx::migrate!("./migrations").run(&pool).await.is_err());
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='trigger' AND name LIKE 'credential_writer_%'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(names, ["credential_writer_accounts_update"]);
    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=61").fetch_one(&pool).await.unwrap();
    assert_eq!(applied, 0);
    let mut old = SqliteConnection::connect_with(&options(&path)).await.unwrap();
    sqlx::query(CHANGE).bind("migration not committed").execute(&mut old).await.unwrap();
    sqlx::query("DROP TRIGGER credential_writer_accounts_update").execute(&pool).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    assert!(missing_protocol(&sqlx::query(CHANGE).bind("now refused").execute(&mut old).await.unwrap_err()));
    old.close().await.unwrap();
    // Independently acquired pooled connections must all be capable, not only the
    // connection selected to run the migration.
    let mut first = pool.acquire().await.unwrap();
    let mut second = pool.acquire().await.unwrap();
    let mut third = pool.acquire().await.unwrap();
    for connection in [&mut first, &mut second, &mut third] {
        sqlx::query("PRAGMA trusted_schema=OFF").execute(&mut **connection).await.unwrap();
        sqlx::query(CHANGE).bind("registered pool writer").execute(&mut **connection).await.unwrap();
    }
    drop((first, second, third));
    pool.close().await;
}

#[tokio::test]
async fn incompatible_registered_comparison_refuses_before_changing_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("incompatible.db");
    let pool = legacy_fixture(&path).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let incompatible = options(&path).collation("rbit_credential_writer_v1", |_, _| std::cmp::Ordering::Less);
    let mut connection = SqliteConnection::connect_with(&incompatible).await.unwrap();
    let error = sqlx::query(CHANGE).bind("refused").execute(&mut connection).await.unwrap_err();
    assert!(error.as_database_error().unwrap().message().contains("credential writer version refused"));
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM user_logins WHERE account_id=1").fetch_one(&pool).await.unwrap();
    assert_eq!(hash, "original");
    connection.close().await.unwrap();
    pool.close().await;
}
