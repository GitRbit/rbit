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

/// Historical fixtures start with virgin policy, so the new cyclic epoch/renewal
/// graph is empty. Drop children before parents with foreign keys still enabled.
async fn drop_policy_graph(data: &Path) {
    use sqlx::{ConnectOptions, Connection};
    let mut connection =
        sqlx::sqlite::SqliteConnectOptions::new().filename(data.join("rbit.db")).foreign_keys(true).connect().await.unwrap();
    sqlx::raw_sql(
        "DROP TABLE plugin_policy_receipts; DROP TABLE plugin_selection_heads; \
         DROP TABLE plugin_review_heads; DROP TABLE plugin_review_history; DROP TABLE plugin_policy_gate; \
         DROP TABLE plugin_policy_renewals; DROP TABLE plugin_policy_epochs",
    )
    .execute(&mut connection)
    .await
    .unwrap();
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

/// Preserve an already sealed fixture; storage tests exercise actual capacity sealing.
async fn seal_policy_fixture(
    data: &Path,
    actor: rbit_core::AccountId,
    mut review: rbit_plugin_api::PackageReviewRequest,
    revision: rbit_plugin_api::PolicyRevision,
) {
    use sqlx::{ConnectOptions, Connection};
    review.expected_review = rbit_plugin_api::ExpectedPolicyRevision::Exact { revision };
    let canonical = rbit_storage::sqlite::plugin_policy::PolicyCommand::Review(review).canonical_request().unwrap();
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new().filename(data.join("rbit.db")).connect().await.unwrap();
    sqlx::query(
        "UPDATE plugin_policy_gate SET state=2,seal_reason=0,seal_actor=?,seal_operation='backup-seal',seal_request=?,sealed_at=1 WHERE id=1",
    )
    .bind(actor.0)
    .bind(canonical)
    .execute(&mut connection)
    .await
    .unwrap();
    connection.close().await.unwrap();
}

#[tokio::test]
async fn backup_restores_exact_review_selection_and_policy_epoch() {
    use rbit_core::Timestamp;
    use rbit_plugin_api::{
        ExpectedPolicyRevision, InstalledPackageRef, PackageReviewIntent, PackageReviewRequest, PackageSelectionRequest,
        PolicyGrantRequests,
    };
    use rbit_storage::{
        NewUser,
        sqlite::{plugin_installations::OperationId, plugin_policy::ApplyOutcome},
    };

    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let (path, bytes) = seed_package(&data).await;
    let database = Database::open(&data.join("rbit.db")).await.unwrap();
    let actor = database
        .accounts()
        .create_user(NewUser {
            name: "policy-admin".parse().unwrap(),
            email: "policy-admin@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused-by-this-storage-fixture".to_owned(),
            is_admin: true,
        })
        .await
        .unwrap();
    let store = database.plugin_policy();
    let gate = store.prepare(actor.id(), Timestamp::now()).await.unwrap();
    let package =
        InstalledPackageRef::new("example.backup".parse().unwrap(), "1.0.0".parse().unwrap(), archive_digest(&bytes).parse().unwrap())
            .unwrap();
    let review = PackageReviewRequest::new(
        package.clone(),
        ExpectedPolicyRevision::Absent {},
        gate.fingerprint.clone().unwrap(),
        PackageReviewIntent::DataOnly {},
        PolicyGrantRequests::new(Vec::new()).unwrap(),
    )
    .unwrap();
    let ApplyOutcome::Applied { receipt, .. } =
        store.apply_review(actor.id(), &OperationId::new("backup-review").unwrap(), &review, Timestamp::now()).await.unwrap()
    else {
        panic!("fresh review must apply");
    };
    let selection = PackageSelectionRequest::new(
        package.clone(),
        ExpectedPolicyRevision::Absent {},
        receipt.result_revision,
        gate.fingerprint.clone().unwrap(),
    );
    assert!(matches!(
        store.apply_selection(actor.id(), &OperationId::new("backup-select").unwrap(), &selection, Timestamp::now()).await.unwrap(),
        ApplyOutcome::Applied { .. }
    ));
    let before = store.inspect(actor.id(), package.plugin_id()).await.unwrap();
    database.close().await;

    let backup = dir.path().join("backup");
    let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(std::fs::read(backup.join(path.strip_prefix(&data).unwrap())).unwrap(), bytes);
    let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
    let after = restored.plugin_policy().inspect(actor.id(), package.plugin_id()).await.unwrap();
    assert_eq!(after.gate.epoch, before.gate.epoch);
    assert_eq!(after.gate.fingerprint, before.gate.fingerprint);
    assert_eq!(after.selection.as_ref().unwrap().package, Some(package));
    assert_eq!(after.selection.as_ref().unwrap().revision, before.selection.as_ref().unwrap().revision);
    assert_eq!(after.review.as_ref().unwrap().revision, before.review.as_ref().unwrap().revision);
    restored.close().await;

    seal_policy_fixture(&data, actor.id(), review, before.review.as_ref().unwrap().revision).await;
    let sealed_backup = dir.path().join("sealed-backup");
    let output = rbit(&data, &["backup", "--output", sealed_backup.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let restored = Database::open(&sealed_backup.join("rbit.db")).await.unwrap();
    let after = restored.plugin_policy().inspect(actor.id(), &"example.backup".parse().unwrap()).await.unwrap();
    assert_eq!(after.gate.state, rbit_storage::sqlite::plugin_policy::PolicyGateState::Sealed);
    assert_eq!(after.gate.epoch, before.gate.epoch);
    assert_eq!(after.gate.fingerprint, before.gate.fingerprint);
    assert_eq!(after.gate.seal.as_ref().unwrap().operation.as_str(), "backup-seal");
    assert_eq!(after.selection.as_ref().unwrap().revision, before.selection.as_ref().unwrap().revision);
    restored.close().await;
    assert_renewed_policy_backup(dir.path(), &data, &path, &bytes, actor.id(), &before).await;
}

#[derive(Debug)]
struct RenewedPolicyBackup {
    source: rbit_storage::sqlite::plugin_policy::PolicyGate,
    renewal: rbit_storage::sqlite::plugin_policy::PolicyRenewalReceipt,
    current: rbit_storage::sqlite::plugin_policy::PolicyPackageSnapshot,
    candidate: rbit_storage::sqlite::plugin_policy::PolicySnapshot,
    ordinary: Vec<rbit_storage::sqlite::plugin_policy::PolicyReceipt>,
}

/// The preceding fixture asserts an already-sealed state; this tests retention
/// and backup, while storage renewal tests exercise real capacity exhaustion.
async fn renew_with_fresh_deny(
    data: &Path,
    actor: rbit_core::AccountId,
    before: &rbit_storage::sqlite::plugin_policy::PolicySnapshot,
) -> RenewedPolicyBackup {
    use rbit_core::Timestamp;
    use rbit_plugin_api::{
        ExpectedPolicyRevision, PackagePolicyRenewalRequest, PackageReviewIntent, PackageReviewRequest, PolicyGrantRequests,
    };
    use rbit_storage::sqlite::{
        plugin_installations::OperationId,
        plugin_policy::{ApplyOutcome, PolicyRenewalOutcome},
    };
    let database = Database::open(&data.join("rbit.db")).await.unwrap();
    let store = database.plugin_policy();
    let source = store.gate(actor).await.unwrap();
    let package = before.selection.as_ref().unwrap().package.as_ref().unwrap();
    let mut ordinary = Vec::new();
    for id in ["backup-review", "backup-select"] {
        ordinary.push(store.operation(actor, &OperationId::new(id).unwrap()).await.unwrap().unwrap());
    }
    let request =
        PackagePolicyRenewalRequest::new(source.fingerprint.clone().unwrap(), source.seal.as_ref().unwrap().operation.as_str()).unwrap();
    let PolicyRenewalOutcome::Applied { receipt: renewal, gate } =
        store.renew(actor, &OperationId::new("backup-renewal").unwrap(), &request, Timestamp::now()).await.unwrap()
    else {
        panic!("sealed fixture must renew once");
    };
    assert_ne!(gate.epoch, source.epoch);
    assert_ne!(gate.fingerprint, source.fingerprint);
    let empty = store.inspect(actor, package.plugin_id()).await.unwrap();
    assert_eq!(empty.gate, gate);
    assert!(empty.selection.is_none());
    assert!(empty.review.is_none());
    assert!(store.inspect_package(actor, package).await.unwrap().review.is_none(), "old allow cannot become a new-epoch review");
    let deny = PackageReviewRequest::new(
        package.clone(),
        ExpectedPolicyRevision::Absent {},
        gate.fingerprint.clone().unwrap(),
        PackageReviewIntent::Deny {},
        PolicyGrantRequests::new(Vec::new()).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.apply_review(actor, &OperationId::new("backup-current-deny").unwrap(), &deny, Timestamp::now()).await.unwrap(),
        ApplyOutcome::Applied { .. }
    ));
    let current = store.inspect_package(actor, package).await.unwrap();
    let candidate = store.inspect(actor, package.plugin_id()).await.unwrap();
    assert_eq!(current.review.as_ref().unwrap().request.intent, PackageReviewIntent::Deny {});
    assert_eq!(current.review.as_ref().unwrap().revision.get(), 1);
    assert!(candidate.selection.is_none());
    assert!(candidate.review.is_none());
    assert_eq!(database.plugin_settings().desired_enabled(package.plugin_id().as_str()).await.unwrap(), None);
    database.close().await;
    RenewedPolicyBackup { source, renewal, current, candidate, ordinary }
}

async fn assert_renewed_policy_backup(
    root: &Path,
    data: &Path,
    archive: &Path,
    bytes: &[u8],
    actor: rbit_core::AccountId,
    before: &rbit_storage::sqlite::plugin_policy::PolicySnapshot,
) {
    use rbit_core::Timestamp;
    use rbit_storage::sqlite::{plugin_installations::OperationId, plugin_policy::ApplyOutcome};
    let expected = renew_with_fresh_deny(data, actor, before).await;
    let backup = root.join("renewed-backup");
    let output = rbit(data, &["backup", "--output", backup.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 5);
    assert_eq!(manifest["plugin_packages"], 1);
    let copied = backup.join(archive.strip_prefix(data).unwrap());
    assert_eq!(std::fs::read(&copied).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(copied.parent().unwrap()).unwrap().count(), 1);
    let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
    let store = restored.plugin_policy();
    let package = &expected.current.package;
    assert_eq!(store.inspect(actor, package.plugin_id()).await.unwrap(), expected.candidate);
    assert_eq!(store.inspect_package(actor, package).await.unwrap(), expected.current);
    assert_eq!(store.renewal_operation(actor, &expected.renewal.id).await.unwrap(), Some(expected.renewal.clone()));
    for receipt in &expected.ordinary {
        assert_eq!(store.operation(actor, &receipt.id).await.unwrap(), Some(receipt.clone()));
    }
    assert!(matches!(
        store
            .apply_review(actor, &OperationId::new("backup-review").unwrap(), &before.review.as_ref().unwrap().request, Timestamp::now())
            .await
            .unwrap(),
        ApplyOutcome::HistoricalReceipt { .. }
    ));
    assert_eq!(store.inspect_package(actor, package).await.unwrap(), expected.current, "replayed old allow cannot replace current deny");
    assert_eq!(restored.plugin_settings().desired_enabled(package.plugin_id().as_str()).await.unwrap(), None);
    assert_retired_policy_metadata(&backup.join("rbit.db"), before, &expected).await;
    restored.close().await;
}

async fn assert_retired_policy_metadata(
    path: &Path,
    before: &rbit_storage::sqlite::plugin_policy::PolicySnapshot,
    expected: &RenewedPolicyBackup,
) {
    use sqlx::{ConnectOptions, Connection, Row};
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new().filename(path).read_only(true).connect().await.unwrap();
    let epoch = expected.source.epoch.as_ref().unwrap();
    let package = before.selection.as_ref().unwrap().package.as_ref().unwrap();
    let registry = sqlx::query("SELECT ordinal,retired,fingerprint,retirement_operation,retired_by,retired_at,seal_operation,seal_request,seal_reason,seal_actor,sealed_at FROM plugin_policy_epochs WHERE epoch=? AND length(CAST(fingerprint AS BLOB))=64 AND length(CAST(retirement_operation AS BLOB)) BETWEEN 1 AND 64 AND length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64 AND length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536 LIMIT 1")
        .bind(epoch.as_str()).fetch_one(&mut connection).await.unwrap();
    assert_eq!(registry.get::<i64, _>("ordinal"), 0);
    assert_eq!(registry.get::<i64, _>("retired"), 1);
    assert_eq!(registry.get::<String, _>("fingerprint"), expected.source.fingerprint.as_ref().unwrap().as_str());
    assert_eq!(registry.get::<String, _>("retirement_operation"), "backup-renewal");
    assert_eq!(registry.get::<Option<i64>, _>("retired_by"), expected.renewal.actor.map(|a| a.0));
    assert_eq!(registry.get::<i64, _>("retired_at"), expected.renewal.created_at.0);
    assert_eq!(registry.get::<String, _>("seal_operation"), "backup-seal");
    assert_eq!(registry.get::<String, _>("seal_request"), expected.source.seal.as_ref().unwrap().request);
    let seal = expected.source.seal.as_ref().unwrap();
    assert_eq!(registry.get::<i64, _>("seal_reason"), 0); // Synthetic HistoryCapacity fixture.
    assert_eq!(registry.get::<Option<i64>, _>("seal_actor"), seal.actor.map(|a| a.0));
    assert_eq!(registry.get::<i64, _>("sealed_at"), seal.at.0);
    let current = sqlx::query("SELECT ordinal,retired,predecessor,created_by_renewal FROM plugin_policy_epochs WHERE epoch=? AND length(CAST(predecessor AS BLOB))=64 AND length(CAST(created_by_renewal AS BLOB)) BETWEEN 1 AND 64 LIMIT 1")
        .bind(expected.renewal.destination_epoch.as_str()).fetch_one(&mut connection).await.unwrap();
    assert_eq!(current.get::<i64, _>("ordinal"), 1);
    assert_eq!(current.get::<i64, _>("retired"), 0);
    assert_eq!(current.get::<String, _>("predecessor"), epoch.as_str());
    assert_eq!(current.get::<String, _>("created_by_renewal"), "backup-renewal");
    let selected = sqlx::query("SELECT revision,version,digest,review_revision,actor,updated_at FROM plugin_selection_heads WHERE epoch=? AND plugin_id=? AND length(CAST(version AS BLOB)) BETWEEN 1 AND 128 AND length(CAST(digest AS BLOB))=64 LIMIT 1")
        .bind(epoch.as_str()).bind(package.plugin_id().as_str()).fetch_one(&mut connection).await.unwrap();
    let previous = before.selection.as_ref().unwrap();
    assert_eq!(selected.get::<i64, _>("revision"), i64::try_from(previous.revision.get()).unwrap());
    assert_eq!(selected.get::<String, _>("version"), package.version().to_string());
    assert_eq!(selected.get::<String, _>("digest"), package.archive_digest().as_str());
    assert_eq!(selected.get::<i64, _>("review_revision"), i64::try_from(previous.review_revision.unwrap().get()).unwrap());
    assert_eq!(selected.get::<Option<i64>, _>("actor"), previous.actor.map(|a| a.0));
    assert_eq!(selected.get::<i64, _>("updated_at"), previous.updated_at.0);
    let reviewed = sqlx::query("SELECT h.request,h.revision,h.actor,h.created_at FROM plugin_review_heads c JOIN plugin_review_history h ON h.epoch=c.epoch AND h.digest=c.digest AND h.revision=c.revision WHERE c.epoch=? AND c.digest=? AND length(CAST(h.request AS BLOB)) BETWEEN 1 AND 65536 LIMIT 1")
        .bind(epoch.as_str()).bind(package.archive_digest().as_str()).fetch_one(&mut connection).await.unwrap();
    let old = before.review.as_ref().unwrap();
    let canonical = rbit_storage::sqlite::plugin_policy::PolicyCommand::Review(old.request.clone()).canonical_request().unwrap();
    assert_eq!(reviewed.get::<String, _>("request"), canonical);
    assert_eq!(reviewed.get::<i64, _>("revision"), i64::try_from(old.revision.get()).unwrap());
    assert_eq!(reviewed.get::<Option<i64>, _>("actor"), old.actor.map(|a| a.0));
    assert_eq!(reviewed.get::<i64, _>("created_at"), old.created_at.0);
    let (epochs, renewals): (i64, i64) =
        sqlx::query_as("SELECT (SELECT COUNT(*) FROM plugin_policy_epochs), (SELECT COUNT(*) FROM plugin_policy_renewals)")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!((epochs, renewals), (2, 1));
    connection.close().await.unwrap();
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
async fn backup_rejects_malformed_inventory_scalars_before_materializing_strings() {
    for sql in [
        "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_objects_immutable; UPDATE plugin_objects SET digest=digest||char(0)||CAST(zeroblob(131072) AS TEXT)",
        "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_installations_pin; UPDATE plugin_installations SET plugin_id=plugin_id||char(0)||CAST(zeroblob(131072) AS TEXT)",
        "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_installations_pin; UPDATE plugin_installations SET version=version||char(0)||CAST(zeroblob(131072) AS TEXT)",
        "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_installations_pin; UPDATE plugin_installations SET digest=digest||char(0)||CAST(zeroblob(131072) AS TEXT)",
        "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_installations_pin; UPDATE plugin_installations SET plugin_id='';",
        // Rebuild just the object table without STRICT to model a malformed
        // snapshot with byte-bounded BLOBs that a String decoder must not reach.
        "CREATE TABLE replacement(digest TEXT PRIMARY KEY,manifest TEXT,wire_bytes INTEGER,recorded_at INTEGER); INSERT INTO replacement SELECT * FROM plugin_objects; DROP TABLE plugin_objects; ALTER TABLE replacement RENAME TO plugin_objects; UPDATE plugin_objects SET digest=CAST(digest AS BLOB)",
        "CREATE TABLE replacement(plugin_id TEXT,version TEXT,digest TEXT,installed_by INTEGER,installed_at INTEGER,PRIMARY KEY(plugin_id,version),UNIQUE(plugin_id,version,digest)); INSERT INTO replacement SELECT * FROM plugin_installations; DROP TABLE plugin_installations; ALTER TABLE replacement RENAME TO plugin_installations; UPDATE plugin_installations SET version=CAST(version AS BLOB)",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        seed_package(&data).await;
        package_sql(&data, sql).await;
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert!(!output.status.success());
        assert!(!backup.join("manifest.json").exists());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("invalid generic plugin package inventory scalar"), "{stderr}");
    }
}

#[tokio::test]
async fn backup_accepts_pre_package_databases_but_rejects_partial_or_missing_current_ledgers() {
    for damage in ["legacy", "partial", "current-missing"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        seed_package(&data).await;
        if damage == "legacy" {
            drop_policy_graph(&data).await;
            package_sql(&data, "DELETE FROM _sqlx_migrations WHERE version >= 63").await;
        }
        package_sql(&data, "DROP TABLE plugin_package_operations").await;
        if damage != "partial" {
            package_sql(&data, "DROP TABLE plugin_installations; DROP TABLE plugin_objects").await;
        }
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert_eq!(output.status.success(), damage == "legacy", "{damage}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(backup.join("manifest.json").exists(), damage == "legacy");
    }
}

#[tokio::test]
async fn backup_accepts_pre_policy_schema_but_rejects_partial_or_missing_current_policy() {
    for damage in ["legacy", "partial", "current-missing"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let (path, bytes) = seed_package(&data).await;
        if damage == "partial" {
            package_sql(&data, "DROP TABLE plugin_policy_receipts").await;
        } else {
            drop_policy_graph(&data).await;
        }
        if damage == "legacy" {
            package_sql(&data, "DROP INDEX plugin_installations_exact; DELETE FROM _sqlx_migrations WHERE version >= 64").await;
        }
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert_eq!(output.status.success(), damage == "legacy", "{damage}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(backup.join("manifest.json").exists(), damage == "legacy");
        if damage == "legacy" {
            assert_eq!(std::fs::read(backup.join(path.strip_prefix(&data).unwrap())).unwrap(), bytes);
            let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
            restored.close().await;
        }
    }
}

#[tokio::test]
async fn backup_accepts_complete_0064_but_rejects_incomplete_0065_even_without_migration_markers() {
    for damage in ["legacy-64", "missing-epochs", "missing-renewals", "missing-both", "orphan-epochs", "orphan-renewals"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let (path, bytes) = seed_package(&data).await;
        if damage == "legacy-64" {
            drop_policy_graph(&data).await;
            package_sql(&data, "DROP INDEX plugin_installations_exact; DELETE FROM _sqlx_migrations WHERE version >= 65").await;
            package_sql(&data, include_str!("../../rbit-storage/migrations/0064_plugin_policy.sql")).await;
        } else {
            // Deliberate corrupt-schema fixtures use the existing FK-disabled
            // mutation helper, rather than pretending these are legal migrations.
            let sql = match damage {
                "missing-epochs" => "DROP TABLE plugin_policy_epochs",
                "missing-renewals" => "DROP TABLE plugin_policy_renewals",
                "missing-both" => "DROP TABLE plugin_policy_renewals; DROP TABLE plugin_policy_epochs",
                "orphan-epochs" => {
                    "DROP TABLE plugin_policy_receipts; DROP TABLE plugin_selection_heads; \
                     DROP TABLE plugin_review_heads; DROP TABLE plugin_review_history; DROP TABLE plugin_policy_gate; \
                     DROP TABLE plugin_policy_renewals; DELETE FROM _sqlx_migrations WHERE version >= 64"
                }
                "orphan-renewals" => {
                    "DROP TABLE plugin_policy_receipts; DROP TABLE plugin_selection_heads; \
                     DROP TABLE plugin_review_heads; DROP TABLE plugin_review_history; DROP TABLE plugin_policy_gate; \
                     DROP TABLE plugin_policy_epochs; DELETE FROM _sqlx_migrations WHERE version >= 64"
                }
                _ => unreachable!(),
            };
            package_sql(&data, sql).await;
        }
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        let accepted = damage == "legacy-64";
        assert_eq!(output.status.success(), accepted, "{damage}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(backup.join("manifest.json").exists(), accepted, "{damage}");
        if accepted {
            assert_eq!(std::fs::read(backup.join(path.strip_prefix(&data).unwrap())).unwrap(), bytes);
            // Real restore migrates the validated old five-table snapshot to0065.
            let restored = Database::open(&backup.join("rbit.db")).await.unwrap();
            restored.close().await;
        }
    }
}

#[tokio::test]
async fn backup_rejects_missing_extra_or_malformed_policy_gate_without_leaking_request_text() {
    for damage in ["missing", "extra", "uninitialized-epoch", "uninitialized-seal"] {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        seed_package(&data).await;
        let sql = match damage {
            "missing" => "DROP TRIGGER plugin_policy_gate_delete; DELETE FROM plugin_policy_gate",
            "extra" => "PRAGMA ignore_check_constraints=ON; INSERT INTO plugin_policy_gate(id,state) VALUES(2,0)",
            "uninitialized-epoch" => {
                "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_policy_gate_transition; UPDATE plugin_policy_gate SET epoch=printf('%064x',1)"
            }
            "uninitialized-seal" => {
                "PRAGMA ignore_check_constraints=ON; DROP TRIGGER plugin_policy_gate_transition; UPDATE plugin_policy_gate SET seal_request='sensitive-policy-fixture'"
            }
            _ => unreachable!(),
        };
        package_sql(&data, sql).await;
        let backup = dir.path().join("backup");
        let output = rbit(&data, &["backup", "--output", backup.to_str().unwrap()]);
        assert!(!output.status.success(), "{damage}");
        assert!(!backup.join("manifest.json").exists(), "{damage}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive-policy-fixture"));
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
