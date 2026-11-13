//! `rbit backup` end to end: build a data directory with a user and two repositories
//! (one with history, one empty), run the real binary, and check that the backup restores.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rbit_core::{Config, Visibility, lfs::LfsOid};
use rbit_services::{CreateRepository, Services, SignUp, releases::ReleaseInput};
use rbit_storage::{Database, KvStore};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// A data directory with user `ada` owning `project` (one commit on `trunk`) and `empty`.
async fn populate(data_dir: &Path) -> (PathBuf, PathBuf) {
    let mut config = Config::default();
    config.storage.data_dir = data_dir.to_owned();
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();

    let signup = SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
    let user = services.accounts.create_user(signup, Some(true)).await.unwrap();
    let mut paths = Vec::new();
    for name in ["project", "empty"] {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility: Visibility::Private };
        services.repositories.create(&user, input).await.unwrap();
        paths.push(services.repositories.open(Some(&user), "ada", name).await.unwrap().disk_path);
    }
    database.close().await;
    std::fs::write(paths[1].join("HEAD"), "ref: refs/heads/trunk\n").unwrap();

    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]);
    std::fs::write(work.path().join("README.md"), "# Project\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "Initial commit"]);
    git(work.path(), &["push", "-q", paths[0].to_str().unwrap(), "HEAD:refs/heads/main", "HEAD:refs/tags/v1"]);
    (paths.remove(0), paths.remove(0))
}

fn rbit(data_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rbit")).args(args).env("RBIT_STORAGE__DATA_DIR", data_dir).env_remove("RBIT_CONFIG").output().unwrap()
}

/// rbit-runner sets `RBIT_ACTIONS=true`, `RBIT_ACTOR`, `RBIT_SHA`, … in every job; a server
/// command run inside a CI job must not read them as settings, while `RBIT_SECTION__KEY`
/// variables still apply.
#[tokio::test]
async fn the_ci_marker_variable_is_not_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    populate(&data_dir).await;
    let backup = dir.path().join("backup");
    let output = Command::new(env!("CARGO_BIN_EXE_rbit"))
        .args(["backup", "--output", backup.to_str().unwrap()])
        .env("RBIT_STORAGE__DATA_DIR", &data_dir)
        .env("RBIT_ACTIONS", "true")
        .env("RBIT_ACTOR", "octocat")
        .env("RBIT_SHA", "0123456789abcdef0123456789abcdef01234567")
        .env("RBIT_ACTIONS__ENABLED", "true")
        .env_remove("RBIT_CONFIG")
        .output()
        .unwrap();
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn backup_writes_a_restorable_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let (project, empty) = populate(&data_dir).await;
    // A sealed value: readable after a restore only with the key from the backup.
    let database = Database::open(&data_dir.join("rbit.db")).await.unwrap();
    database.server_secrets().get_or_insert("probe", b"sealed value").await.unwrap();
    database.close().await;
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));

    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 6);
    assert_eq!(manifest["secret_key"], "secret.key");
    assert_eq!(std::fs::read(backup.join("secret.key")).unwrap(), std::fs::read(data_dir.join("secret.key")).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(backup.clone()), 0o700, "the backup directory is private");
        for file in ["rbit.db", "secret.key", "manifest.json"] {
            assert_eq!(mode(backup.join(file)), 0o600, "{file} is private");
        }
    }
    assert_eq!(manifest["accounts"], 1);
    assert_eq!(manifest["repositories"], 2);
    assert_eq!(manifest["missing_repositories"], serde_json::json!([]));
    assert!(manifest["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(!backup.join("rbit.redb").exists(), "the key/value store is deliberately skipped");

    // The backup mirrors the data directory layout, so restoring is a plain copy.
    let relative = |path: &Path| path.strip_prefix(&data_dir).unwrap().to_owned();
    let copied_project = backup.join(relative(&project));
    let copied_empty = backup.join(relative(&empty));
    let git_dir = |path: &Path| format!("--git-dir={}", path.display());
    assert_eq!(
        git(dir.path(), &[&git_dir(&copied_project), "show-ref"]),
        git(dir.path(), &[&git_dir(&project), "show-ref"]),
        "every ref is copied"
    );
    git(dir.path(), &[&git_dir(&copied_project), "fsck", "--strict"]);
    assert_eq!(std::fs::read_to_string(copied_project.join("HEAD")).unwrap(), "ref: refs/heads/main\n");
    assert_eq!(std::fs::read_to_string(copied_empty.join("HEAD")).unwrap(), "ref: refs/heads/trunk\n");
    let config = std::fs::read_to_string(copied_project.join("config")).unwrap();
    assert!(!config.contains("origin"), "the copy must not point back at the source:\n{config}");

    // The snapshot opens as a regular database (migrations are already applied).
    let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
    let repos = restored.admin().list_repositories(rbit_storage::sqlite::admin::ListFilter { search: "", limit: 10, offset: 0 }).await;
    assert_eq!(repos.unwrap().total, 2);
    assert_eq!(restored.server_secrets().get_or_insert("probe", b"other").await.unwrap(), b"sealed value");
    restored.close().await;

    // Never overwrite an existing backup.
    let again = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("already exists"));
}

#[tokio::test]
async fn backup_reports_repositories_missing_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let (_, empty) = populate(&data_dir).await;
    std::fs::remove_dir_all(&empty).unwrap();
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["repositories"], 1);
    assert_eq!(manifest["missing_repositories"].as_array().unwrap().len(), 1);
    // The snapshot leaves the repository out too, so a restore never refers to it.
    let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
    let repos = restored.admin().list_repositories(rbit_storage::sqlite::admin::ListFilter { search: "", limit: 10, offset: 0 }).await;
    assert_eq!(repos.unwrap().total, 1);
    restored.close().await;
}

/// The rows of `table` in the backup's database (`SQLite` identifiers are fixed here).
async fn rows(backup: &Path, table: &str) -> i64 {
    use sqlx::{ConnectOptions, Connection};
    let mut connection =
        sqlx::sqlite::SqliteConnectOptions::new().filename(backup.join("rbit.db")).read_only(true).connect().await.unwrap();
    let count = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"))).fetch_one(&mut connection).await.unwrap();
    connection.close().await.unwrap();
    count
}

#[tokio::test]
async fn a_release_file_deleted_before_its_copy_leaves_no_row_behind() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    populate(&data_dir).await;
    // What a backup sees when the release is deleted between its snapshot and the copy.
    std::fs::remove_file(attach_release_file(&data_dir).await).unwrap();
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["missing_attachments"].as_array().unwrap().len(), 1);
    assert_eq!(rows(&backup, "release_assets").await, 0, "the row of the missing file is left out");
    assert_eq!(rows(&backup, "releases").await, 1, "the release itself stays");
}

#[tokio::test]
async fn a_key_kept_outside_the_data_directory_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    populate(&data_dir).await;
    let key_file = dir.path().join("elsewhere.key");
    std::fs::copy(data_dir.join("secret.key"), &key_file).unwrap();
    let backup = dir.path().join("backup");

    let output = Command::new(env!("CARGO_BIN_EXE_rbit"))
        .args(["backup", "--output", backup.to_str().unwrap()])
        .env("RBIT_STORAGE__DATA_DIR", &data_dir)
        .env("RBIT_STORAGE__SECRET_KEY_FILE", &key_file)
        .env_remove("RBIT_CONFIG")
        .output()
        .unwrap();
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest["secret_key"].as_str().unwrap().starts_with("not included"));
    assert!(!backup.join("secret.key").exists(), "a separately kept key stays separate");
}

/// Attaches `app.zip` to a release of `ada/project`'s tag `v1` and returns its path.
async fn attach_release_file(data_dir: &Path) -> PathBuf {
    let mut config = Config::default();
    config.storage.data_dir = data_dir.to_owned();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let attachments = config.storage.attachments_dir();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let id = database.accounts().find_account_by_name("ada").await.unwrap().unwrap().id;
    let ada = database.accounts().find_user(id).await.unwrap().unwrap();
    let repo = services.repositories.open(Some(&ada), "ada", "project").await.unwrap();
    let input = ReleaseInput {
        tag_name: "v1".into(),
        name: String::new(),
        notes: String::new(),
        is_draft: false,
        is_prerelease: false,
        target: String::new(),
    };
    let release = services.releases.create(&repo, &ada, input).await.unwrap();
    let asset = services.releases.upload_asset(&repo, &ada, release.id, "app.zip", "application/zip", &b"release bytes"[..]).await.unwrap();
    database.close().await;
    attachments.join(repo.repo.repo.id.0.to_string()).join(asset.id.0.to_string())
}

#[tokio::test]
async fn backup_includes_release_files_but_not_uploads_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    populate(&data_dir).await;
    let file = attach_release_file(&data_dir).await;
    let tmp = data_dir.join("attachments").join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("half.part"), "unfinished").unwrap();
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!((manifest["attachments"].as_u64(), manifest["missing_attachments"].as_array().map(Vec::len)), (Some(1), Some(0)));

    // Restoring is a copy, so the file sits at the same relative path.
    let copied = backup.join(file.strip_prefix(&data_dir).unwrap());
    assert_eq!(std::fs::read(&copied).unwrap(), b"release bytes");
    assert!(!backup.join("attachments").join("tmp").exists(), "uploads in progress are skipped");
}

/// Milestone 3: Git LFS (L). Stores one object for `ada/project` and returns its path.
async fn store_lfs_object(data_dir: &Path, content: &[u8]) -> PathBuf {
    let mut config = Config::default();
    config.storage.data_dir = data_dir.to_owned();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let lfs_dir = config.storage.lfs_dir();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let id = database.accounts().find_account_by_name("ada").await.unwrap().unwrap().id;
    let ada = database.accounts().find_user(id).await.unwrap().unwrap();
    let repo = services.repositories.open(Some(&ada), "ada", "project").await.unwrap();
    let oid = LfsOid::from_digest(<sha2::Sha256 as sha2::Digest>::digest(content).into());
    services.lfs.store(&repo, &oid, content.len() as u64, content).await.unwrap();
    database.close().await;
    let (first, second) = oid.shards();
    lfs_dir.join(first).join(second).join(oid.as_str())
}

#[tokio::test]
async fn backup_includes_lfs_objects() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    populate(&data_dir).await;
    let object = store_lfs_object(&data_dir, b"large file content").await;
    let gone = store_lfs_object(&data_dir, b"removed before the backup").await;
    std::fs::remove_file(&gone).unwrap();
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["lfs_objects"], 1);
    let missing = gone.file_name().unwrap().to_str().unwrap();
    assert_eq!(manifest["missing_lfs_objects"], serde_json::json!([missing]));

    let copied = backup.join(object.strip_prefix(&data_dir).unwrap());
    assert_eq!(std::fs::read(&copied).unwrap(), b"large file content");
    assert_eq!(rows(&backup, "lfs_objects").await, 1, "the missing object's row is left out");
}

/// The highest migration `version` recorded in the database at `path`.
async fn schema(path: &Path) -> i64 {
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path).read_only(true);
    let mut connection = sqlx::ConnectOptions::connect(&options).await.unwrap();
    let version = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations").fetch_one(&mut connection).await.unwrap();
    sqlx::Connection::close(connection).await.unwrap();
    version
}

#[tokio::test]
async fn backup_of_an_older_database_leaves_it_unmigrated() {
    // A data directory last served by the previous release (migration 59), backed up with
    // this binary before the upgrade: neither it nor the backup may be migrated, or the
    // backup no longer restores for the previous release (the rollback path).
    const PREVIOUS_RELEASE: i64 = 59;
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(data_dir.join("repositories")).unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(data_dir.join("rbit.db")).create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    sqlx::migrate!("../rbit-storage/migrations").run_to(PREVIOUS_RELEASE, &pool).await.unwrap();
    pool.close().await;
    let backup = dir.path().join("backup");

    let output = rbit(&data_dir, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "backup failed: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(schema(&data_dir.join("rbit.db")).await, PREVIOUS_RELEASE, "the live database is not migrated");
    assert_eq!(schema(&backup.join("rbit.db")).await, PREVIOUS_RELEASE, "the backup keeps the previous release's schema");

    let restored = dir.path().join("restored");
    let output = rbit(&restored, &["restore", "--from", backup.to_str().unwrap()]);
    assert!(output.status.success(), "restore failed: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(schema(&restored.join("rbit.db")).await, PREVIOUS_RELEASE, "restoring does not migrate either");
}
