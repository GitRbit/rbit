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
    assert_eq!(manifest["format"], 5);
    assert_eq!(manifest["plugin_packages"], 0);
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
}

fn archive_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in sha2::Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

/// Real canonical package bytes and their typed, passive ledger declaration.
fn package_fixture() -> (Vec<u8>, rbit_plugin_api::PackageManifest) {
    let payload = b"generic plugin backup payload";
    let manifest = serde_json::json!({
        "schema_version":1,
        "plugin":{"id":"example.backup", "version":"1.0.0", "host_api":"^1"},
        "kind":{"type":"data"},
        "files":[{"path":"data/file", "size":payload.len(), "sha256":archive_digest(payload)}],
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    });
    let json = serde_json::to_vec(&manifest).unwrap();
    let mut bytes = Vec::new();
    for (path, body) in [("package.json", json.as_slice()), ("data/file", payload.as_slice())] {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(body);
        bytes.resize(bytes.len().next_multiple_of(512), 0);
    }
    bytes.resize(bytes.len() + 1024, 0);
    (bytes, serde_json::from_value(manifest).unwrap())
}

async fn seed_package(data: &Path) -> (PathBuf, Vec<u8>) {
    use rbit_storage::sqlite::plugin_installations::{InstallationDescriptor, OperationId};
    std::fs::create_dir_all(data).unwrap();
    let database = Database::open(&data.join("rbit.db")).await.unwrap();
    let (bytes, manifest) = package_fixture();
    let descriptor = InstallationDescriptor::new(manifest, archive_digest(&bytes).parse().unwrap(), bytes.len() as u64).unwrap();
    let store = database.plugin_installations();
    for token in ["backup-first", "backup-shared-object"] {
        let token = OperationId::new(token).unwrap();
        store.begin(&token, &descriptor, None, rbit_core::Timestamp::now()).await.unwrap();
        store.complete(&token, 0, rbit_core::Timestamp::now()).await.unwrap();
    }
    // Pending intent carries no durable object assertion, so its absent archive
    // must not be mistaken for installed state during backup.
    let mut pending_manifest = descriptor.manifest().clone();
    pending_manifest.plugin.id = "example.pending".parse().unwrap();
    let pending = InstallationDescriptor::new(pending_manifest, "f".repeat(64).parse().unwrap(), 1024).unwrap();
    store.begin(&OperationId::new("pending").unwrap(), &pending, None, rbit_core::Timestamp::now()).await.unwrap();
    database.close().await;
    let root = data.join("plugins").join("packages");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{}.rbit-plugin", archive_digest(&bytes)));
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

async fn package_sql(data: &Path, sql: &'static str) {
    use sqlx::{ConnectOptions, Connection};
    let mut connection =
        sqlx::sqlite::SqliteConnectOptions::new().filename(data.join("rbit.db")).foreign_keys(false).connect().await.unwrap();
    sqlx::raw_sql(sql).execute(&mut connection).await.unwrap();
    connection.close().await.unwrap();
}

#[tokio::test]
async fn backup_restores_retained_package_pins_and_copies_shared_objects_once() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let (path, bytes) = seed_package(&data).await;
    // Unreferenced staging and CAS objects do not belong to the durable ledger.
    std::fs::write(path.parent().unwrap().join("not-in-ledger.rbit-plugin"), b"unreferenced").unwrap();
    let backup = dir.path().join("backup");
    let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 5);
    assert_eq!(manifest["plugin_packages"], 1);
    let copied = backup.join(path.strip_prefix(&data).unwrap());
    assert_eq!(std::fs::read(&copied).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(copied.parent().unwrap()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&copied).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
    let installed =
        restored.plugin_installations().installation(&"example.backup".parse().unwrap(), &"1.0.0".parse().unwrap()).await.unwrap().unwrap();
    assert_eq!(installed.descriptor.digest().as_str(), archive_digest(&bytes));
    assert_eq!(restored.plugin_installations().objects(None).await.unwrap().len(), 1);
    restored.close().await;
}

#[tokio::test]
async fn backup_includes_retained_package_objects_without_an_installation_pin() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let (path, bytes) = seed_package(&data).await;
    package_sql(&data, "DELETE FROM plugin_installations").await;
    let backup = dir.path().join("backup");
    let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read(backup.join(path.strip_prefix(&data).unwrap())).unwrap(), bytes);
}

#[tokio::test]
async fn backup_fails_before_final_manifest_for_missing_or_corrupt_package_objects() {
    for damage in ["missing", "digest", "short", "trailing", "directory"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let (path, mut bytes) = seed_package(&data).await;
        match damage {
            "missing" => std::fs::remove_file(&path).unwrap(),
            "digest" => {
                bytes[1024] ^= 1;
                std::fs::write(&path, &bytes).unwrap();
            }
            "short" => std::fs::write(&path, b"short").unwrap(),
            "trailing" => {
                bytes.push(0);
                std::fs::write(&path, &bytes).unwrap();
            }
            "directory" => {
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            _ => unreachable!(),
        }
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert!(!output.status.success(), "{damage}");
        assert!(!backup.join("manifest.json").exists(), "{damage}");
    }
}

#[tokio::test]
async fn backup_rejects_missing_or_mismatched_ledger_pins_and_arbitrary_digest_paths() {
    for damage in ["missing-object-row", "wrong-identity", "digest-path", "manifest-json", "capacity"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        seed_package(&data).await;
        let sql = match damage {
            "missing-object-row" => "DELETE FROM plugin_objects",
            "wrong-identity" => "DROP TRIGGER plugin_installations_pin; UPDATE plugin_installations SET plugin_id = 'example.other'",
            "digest-path" => {
                "PRAGMA ignore_check_constraints = ON; DROP TRIGGER plugin_objects_immutable; UPDATE plugin_objects SET digest = '../outside'"
            }
            "manifest-json" => {
                "DROP TRIGGER plugin_objects_immutable; UPDATE plugin_objects SET manifest = '{\"sensitive-unknown-field\":true}'"
            }
            "capacity" => {
                "DROP TRIGGER plugin_objects_capacity; WITH RECURSIVE tally(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM tally WHERE x < 1024) INSERT INTO plugin_objects (digest, manifest, wire_bytes, recorded_at) SELECT printf('%064x', x), '{}', 1, 0 FROM tally"
            }
            _ => unreachable!(),
        };
        package_sql(&data, sql).await;
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert!(!output.status.success(), "{damage}");
        assert!(!backup.join("manifest.json").exists(), "{damage}");
        assert!(!dir.path().join("outside").exists());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive-unknown-field"));
        if damage == "capacity" {
            assert!(String::from_utf8_lossy(&output.stderr).contains("inventory exceeds its bound"));
        }
    }
}

#[tokio::test]
async fn backup_accepts_pre_package_databases_but_rejects_partial_or_missing_current_ledgers() {
    for damage in ["legacy", "partial", "current-missing"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        seed_package(&data).await;
        package_sql(&data, "DROP TABLE plugin_package_operations").await;
        if damage != "partial" {
            package_sql(&data, "DROP TABLE plugin_installations; DROP TABLE plugin_objects").await;
        }
        if damage == "legacy" {
            package_sql(&data, "DELETE FROM _sqlx_migrations WHERE version >= 63").await;
        }
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert_eq!(output.status.success(), damage == "legacy", "{damage}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(backup.join("manifest.json").exists(), damage == "legacy");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn backup_rejects_symlinked_package_files_and_directory_components() {
    for component in ["file", "packages", "plugins"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let (path, bytes) = seed_package(&data).await;
        let original = match component {
            "file" => path,
            "packages" => data.join("plugins").join("packages"),
            "plugins" => data.join("plugins"),
            _ => unreachable!(),
        };
        let outside = dir.path().join("outside");
        std::fs::rename(&original, &outside).unwrap();
        std::os::unix::fs::symlink(&outside, &original).unwrap();
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert!(!output.status.success(), "{component}");
        assert!(!backup.join("manifest.json").exists());
        if component == "file" {
            assert_eq!(std::fs::read(&outside).unwrap(), bytes);
        }
    }
}
