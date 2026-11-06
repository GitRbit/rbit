//! Closed synthetic snapshots only. No configuration, environment or user database discovery.
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    fs,
    process::{Command, Output},
};
fn run(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rbit-credential-inventory")).args(arguments).output().unwrap()
}
#[test]
fn explicit_snapshot_is_required_and_errors_do_not_echo_paths() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("private-identity-do-not-echo.sqlite");
    for arguments in [
        vec![],
        vec![std::ffi::OsStr::new("--snapshot")],
        vec![std::ffi::OsStr::new("--snapshot"), missing.as_os_str()],
        vec![std::ffi::OsStr::new("--other"), missing.as_os_str()],
    ] {
        let output = run(&arguments);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8(output.stderr).unwrap().contains("private-identity"));
        assert!(!missing.exists());
    }
}
#[tokio::test]
async fn cli_json_is_only_metadata_and_creates_no_sidecars() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-identity.sqlite");
    let mut db = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path).create_if_missing(true)).await.unwrap();
    sqlx::query("CREATE TABLE user_logins (password_hash TEXT)").execute(&mut db).await.unwrap();
    sqlx::query("INSERT INTO user_logins VALUES ('private-credential-string')").execute(&mut db).await.unwrap();
    db.close().await.unwrap();
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let output = run(&[std::ffi::OsStr::new("--snapshot"), path.as_os_str()]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json = String::from_utf8(output.stdout).unwrap();
    assert!(!json.contains("private"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["rows"], 1);
    assert_eq!(value["counts"]["malformed"], 1);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
