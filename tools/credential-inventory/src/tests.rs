use super::*;
use std::{collections::BTreeMap, fs, path::PathBuf};
use tempfile::TempDir;
const VECTOR: &str = "$argon2id$v=19$m=256,t=2,p=2$c29tZXNhbHQ$bQk8UB/VmZZF4Oo79iDXuL5/0ttZwg2f/5U52iv1cDc";
async fn fixture(values: &[&str]) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-usernames.sqlite");
    let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let mut db = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TABLE user_logins (account_id INTEGER, email TEXT, password_hash TEXT)").execute(&mut db).await.unwrap();
    for value in values {
        sqlx::query("INSERT INTO user_logins VALUES (777,'private-email',?)").bind(value).execute(&mut db).await.unwrap();
    }
    db.close().await.unwrap();
    (directory, path)
}
fn files(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.file_name().unwrap().into(), fs::read(path).unwrap())
        })
        .collect()
}
#[tokio::test]
async fn closed_snapshot_aggregate_does_not_mutate_files_or_publish_secrets() {
    let oversized = "PRIVATE_OVERSIZED_PHC".repeat(10000);
    let (directory, path) = fixture(&[VECTOR, VECTOR, "private-malformed", &oversized]).await;
    let before = files(directory.path());
    let report = scan_snapshot(&path).await.unwrap();
    assert_eq!(report.rows, 4);
    assert_eq!(report.counts.parameters_observed, 2);
    assert_eq!((report.counts.malformed, report.counts.oversized), (1, 1));
    assert!(report.complete);
    assert_eq!(report.phc_bytes.maximum, Some(oversized.len() as u64));
    assert_eq!(before, files(directory.path()));
    let json = serde_json::to_string(&report).unwrap();
    for secret in [VECTOR, "private", "777", "c29tZXNhbHQ", "bQk8UB", "OVERSIZED"] {
        assert!(!json.contains(secret));
    }
}
#[tokio::test]
async fn each_sidecar_is_refused_before_and_after_scanning_without_modification() {
    for suffix in ["-wal", "-shm", "-journal"] {
        let (directory, path) = fixture(&[VECTOR]).await;
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let sidecar = PathBuf::from(name);
        fs::write(&sidecar, b"private-sidecar").unwrap();
        let before = files(directory.path());
        assert_eq!(scan_snapshot(&path).await.unwrap_err(), ScanError::Sidecar);
        assert_eq!(before, files(directory.path()));
        fs::remove_file(&sidecar).unwrap();
        let error = scan_with_hook(&path, Limits::default(), |_| {
            fs::write(&sidecar, b"private-sidecar").unwrap();
        })
        .await
        .unwrap_err();
        assert_eq!(error, ScanError::Sidecar);
    }
}
#[tokio::test]
async fn file_content_drift_invalidates_output_instead_of_publishing_partial_report() {
    let (_directory, path) = fixture(&[VECTOR]).await;
    let error = scan_with_hook(&path, Limits::default(), |path| {
        let mut bytes = fs::read(path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(path, bytes).unwrap();
    })
    .await
    .unwrap_err();
    assert_eq!(error, ScanError::Drift);
}
#[tokio::test]
async fn row_and_group_limits_have_explicit_incomplete_reports() {
    let second = VECTOR.replace("t=2", "t=3");
    let (_directory, path) = fixture(&[VECTOR, &second, VECTOR]).await;
    let report = scan_with_hook(&path, Limits { rows: 1, ..Limits::default() }, |_| {}).await.unwrap();
    assert_eq!(report.rows, 1);
    assert!(report.row_limit_reached);
    assert!(!report.complete);
    let report = scan_with_hook(&path, Limits { groups: 1, ..Limits::default() }, |_| {}).await.unwrap();
    assert_eq!(report.rows, 3);
    assert_eq!(report.counts.groups_omitted, 1);
    assert!(!report.complete);
}
#[tokio::test]
async fn absent_input_is_not_created_and_a_view_is_not_executed() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.sqlite");
    assert_eq!(scan_snapshot(&missing).await.unwrap_err(), ScanError::Input);
    assert!(!missing.exists());
    let path = directory.path().join("view.sqlite");
    let mut db = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path).create_if_missing(true)).await.unwrap();
    sqlx::query("CREATE VIEW user_logins AS SELECT 'secret' AS password_hash").execute(&mut db).await.unwrap();
    db.close().await.unwrap();
    let before = files(directory.path());
    assert_eq!(scan_snapshot(&path).await.unwrap_err(), ScanError::Schema);
    assert_eq!(before, files(directory.path()));
}
#[tokio::test]
async fn invalid_storage_is_counted_without_reading_its_value() {
    let (_directory, path) = fixture(&[VECTOR]).await;
    let mut db = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await.unwrap();
    sqlx::query("INSERT INTO user_logins VALUES(999,'secret',X'70726976617465')").execute(&mut db).await.unwrap();
    db.close().await.unwrap();
    let report = scan_snapshot(&path).await.unwrap();
    assert_eq!(report.counts.invalid_storage, 1);
}

#[tokio::test]
async fn clean_closed_wal_mode_snapshot_never_creates_wal_or_shm() {
    let (directory, path) = fixture(&[VECTOR]).await;
    let mut db = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await.unwrap();
    sqlx::query("PRAGMA journal_mode=WAL").execute(&mut db).await.unwrap();
    db.close().await.unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    let before = files(directory.path());
    let report = scan_snapshot(&path).await.unwrap();
    assert_eq!(report.counts.parameters_observed, 1);
    assert_eq!(before, files(directory.path()));
}

#[tokio::test]
async fn invalid_database_errors_stay_fixed_and_input_is_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-user.sqlite");
    fs::write(&path, b"private credential bytes in an invalid database").unwrap();
    let before = files(directory.path());
    let error = scan_snapshot(&path).await.unwrap_err();
    assert!(matches!(error, ScanError::Database | ScanError::Schema));
    assert!(!error.code().contains("private"));
    assert_eq!(before, files(directory.path()));
}

#[cfg(unix)]
#[tokio::test]
async fn linked_input_and_linked_ancestors_are_refused_without_writes() {
    let (directory, path) = fixture(&[VECTOR]).await;
    let linked_file = directory.path().join("linked.sqlite");
    std::os::unix::fs::symlink(&path, &linked_file).unwrap();
    assert_eq!(scan_snapshot(&linked_file).await.unwrap_err(), ScanError::Input);
    let holder = tempfile::tempdir().unwrap();
    let linked_parent = holder.path().join("linked-directory");
    std::os::unix::fs::symlink(directory.path(), &linked_parent).unwrap();
    assert_eq!(scan_snapshot(&linked_parent.join(path.file_name().unwrap())).await.unwrap_err(), ScanError::Input);
    assert_eq!(scan_snapshot(&path).await.unwrap().rows, 1);
}

#[tokio::test]
async fn utf16_snapshots_are_refused_instead_of_misclassifying_valid_phc_text() {
    for pragma in ["PRAGMA encoding='UTF-16le'", "PRAGMA encoding='UTF-16be'"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("encoding.sqlite");
        let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
        let mut database = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::query(pragma).execute(&mut database).await.unwrap();
        sqlx::query("CREATE TABLE user_logins (password_hash TEXT)").execute(&mut database).await.unwrap();
        sqlx::query("INSERT INTO user_logins VALUES (?)").bind(VECTOR).execute(&mut database).await.unwrap();
        database.close().await.unwrap();
        let before = files(directory.path());
        assert_eq!(scan_snapshot(&path).await.unwrap_err(), ScanError::Schema);
        assert_eq!(before, files(directory.path()));
    }
}
