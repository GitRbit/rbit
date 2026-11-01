//! Host-only package installation: verified immutable bytes, journal and owned work.
use std::{
    fmt::Write as _,
    fs,
    io::{self, Cursor, Read},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rbit_core::{Config, User};
#[cfg(windows)]
use rbit_plugin_packages::Durability;
use rbit_plugin_packages::{MAX_ARCHIVE_WIRE_BYTES, StoreError, StoreFailure};
use rbit_services::{PackageInstallOutcome, ServiceError, Services};
use rbit_storage::{
    Database, KvStore, NewUser,
    sqlite::plugin_installations::{InstallationOperation, OperationId, OperationStage},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    _kv: KvStore,
    services: Services,
    root: PathBuf,
    admin: User,
    reader: User,
}
impl Fixture {
    async fn new(provision: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.updates.check_for_updates = false;
        config.actions.enabled = false;
        config.i18n.enabled = false;
        fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let root = config.storage.data_dir.join("plugins").join("packages");
        if provision {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&root).unwrap();
            #[cfg(unix)]
            for directory in [&root, &root.parent().unwrap().to_path_buf(), &config.storage.data_dir] {
                fs::File::open(directory).unwrap().sync_all().unwrap();
            }
        }
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = database.open_kv(&config.storage.kv_path()).unwrap();
        let mut users = Vec::new();
        for (name, is_admin) in [("alice", true), ("bob", false)] {
            users.push(
                database
                    .accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "unused".to_owned(),
                        is_admin,
                    })
                    .await
                    .unwrap(),
            );
        }
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let reader = users.pop().unwrap();
        let admin = users.pop().unwrap();
        Self { _directory: directory, database, _kv: kv, services, root, admin, reader }
    }
}
fn hash(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}
fn archive(kind: &Value, payload: &[u8], version: &str) -> Vec<u8> {
    let descriptor = serde_json::to_vec(&json!({
        "schema_version":1,
        "plugin":{"id":"example.package", "version":version, "host_api":"^1", "capabilities":["example.read"]},
        "kind":kind,
        "files":[{"path":"payload.bin", "size":payload.len(), "sha256":hash(payload)}],
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    }))
    .unwrap();
    let mut bytes = Vec::new();
    for (path, body) in [("package.json", descriptor.as_slice()), ("payload.bin", payload)] {
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
    bytes
}
fn data() -> Vec<u8> {
    archive(&json!({"type":"data"}), b"immutable content", "1.0.0")
}
fn id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_mins(1)
}
fn store_failure(error: &ServiceError) -> &StoreFailure {
    let ServiceError::Internal(error) = error else { panic!("unexpected error: {error:?}") };
    error.downcast_ref::<StoreFailure>().unwrap()
}

struct Probe(Arc<AtomicBool>);
impl Read for Probe {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.0.store(true, Ordering::Release);
        Ok(0)
    }
}

#[tokio::test]
async fn construction_is_lazy_and_authorization_precedes_source_and_store_work() {
    let fixture = Fixture::new(false).await;
    assert!(!fixture.root.exists());
    let read = Arc::new(AtomicBool::new(false));
    let error = fixture
        .services
        .packages
        .install(&fixture.reader, id("forbidden"), Probe(Arc::clone(&read)), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap_err();
    assert!(matches!(error, ServiceError::Forbidden));
    assert!(matches!(fixture.services.packages.operation(&fixture.reader, &id("forbidden")).await, Err(ServiceError::Forbidden)));
    assert!(!read.load(Ordering::Acquire));
    assert!(!fixture.root.exists());
    let error = fixture
        .services
        .packages
        .install(&fixture.admin, id("missing-root"), Cursor::new(data()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap_err();
    assert!(matches!(error, ServiceError::Internal(_)));
    assert!(!fixture.root.exists());
    assert!(fixture.database.plugin_installations().operation(&id("missing-root")).await.unwrap().is_none());
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn data_native_and_wasm_are_installed_disabled_without_extraction_or_execution() {
    let fixture = Fixture::new(true).await;
    for (token, kind, version) in [
        ("data", json!({"type":"data"}), "1.0.0"),
        ("native", json!({"type":"native", "target":"x86_64-unknown-linux-gnu", "entrypoint":"payload.bin"}), "2.0.0"),
        ("wasm", json!({"type":"wasm", "entrypoint":"payload.bin"}), "3.0.0"),
    ] {
        // Deliberately not executable code: installation retains declarations and
        // never attempts to load, execute or validate an executable format.
        let bytes = archive(&kind, b"not executable", version);
        let outcome = fixture
            .services
            .packages
            .install(&fixture.admin, id(token), Cursor::new(bytes.clone()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
            .await
            .unwrap();
        let PackageInstallOutcome::Installed(installed) = outcome else { panic!("new publication must be durable") };
        assert_eq!(installed.descriptor.digest().as_str(), hash(&bytes));
        assert_eq!(fs::read(fixture.root.join(format!("{}.rbit-plugin", hash(&bytes)))).unwrap(), bytes);
        let operation = fixture.services.packages.operation(&fixture.admin, &id(token)).await.unwrap().unwrap();
        assert_eq!(operation.stage, OperationStage::Complete);
        assert_eq!(operation.actor, Some(fixture.admin.id()));
        assert_eq!(installed.descriptor.manifest().plugin.capabilities[0].as_str(), "example.read");
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 3);
    assert!(!fixture.root.join("payload.bin").exists());
    assert_eq!(fixture.database.plugin_settings().desired_enabled("example.package").await.unwrap(), None);
    assert!(fixture.services.actions.is_none());
    assert!(!fixture.services.i18n_enabled());
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn exact_replays_verify_bytes_and_pin_digests_without_rewriting_identity() {
    let fixture = Fixture::new(true).await;
    let bytes = data();
    let token = id("install");
    let installed = fixture
        .services
        .packages
        .install(&fixture.admin, token.clone(), Cursor::new(bytes.clone()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap();
    let PackageInstallOutcome::Installed(first) = installed else { panic!("new publication") };
    for token in [token.clone(), id("another-token")] {
        let outcome = fixture
            .services
            .packages
            .install(&fixture.admin, token, Cursor::new(bytes.clone()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
            .await
            .unwrap();
        let PackageInstallOutcome::Installed(replayed) = outcome else { panic!("prior durable ledger record") };
        assert_eq!(replayed, first);
    }
    let different = archive(&json!({"type":"data"}), b"different content", "1.0.0");
    assert!(matches!(
        fixture
            .services
            .packages
            .install(&fixture.admin, id("different"), Cursor::new(different), deadline(), MAX_ARCHIVE_WIRE_BYTES)
            .await,
        Err(ServiceError::Conflict(_))
    ));
    let path = fixture.root.join(format!("{}.rbit-plugin", hash(&bytes)));
    let mut corrupt = bytes.clone();
    corrupt[1024] ^= 1;
    fs::write(&path, &corrupt).unwrap();
    let error =
        fixture.services.packages.install(&fixture.admin, token, Cursor::new(bytes), deadline(), MAX_ARCHIVE_WIRE_BYTES).await.unwrap_err();
    assert_eq!(store_failure(&error).error(), StoreError::CorruptExisting);
    assert_eq!(fs::read(path).unwrap(), corrupt);
    assert_eq!(fixture.services.packages.operation(&fixture.admin, &id("install")).await.unwrap().unwrap().stage, OperationStage::Complete);
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn invalid_sources_and_bounds_create_no_journal_or_published_object() {
    let fixture = Fixture::new(true).await;
    let error = fixture
        .services
        .packages
        .install(&fixture.admin, id("invalid"), Cursor::new(b"not tar".to_vec()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap_err();
    assert!(matches!(store_failure(&error).error(), StoreError::Archive(_)));
    assert!(fixture.services.packages.operation(&fixture.admin, &id("invalid")).await.unwrap().is_none());
    for (deadline, quota) in [(Instant::now(), MAX_ARCHIVE_WIRE_BYTES), (deadline(), 0), (deadline(), MAX_ARCHIVE_WIRE_BYTES + 1)] {
        assert!(matches!(
            fixture.services.packages.install(&fixture.admin, id("bounds"), Cursor::new(data()), deadline, quota).await,
            Err(ServiceError::Invalid(_))
        ));
    }
    assert_eq!(fixture.database.plugin_installations().recovery(None).await.unwrap(), Vec::<InstallationOperation>::new());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn uncertain_existing_publication_requires_exact_prior_durable_ledger_reference() {
    let fixture = Fixture::new(true).await;
    let bytes = data();
    fs::write(fixture.root.join(format!("{}.rbit-plugin", hash(&bytes))), &bytes).unwrap();
    let outcome = fixture
        .services
        .packages
        .install(&fixture.admin, id("preexisting"), Cursor::new(bytes), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap();
    #[cfg(unix)]
    assert!(matches!(outcome, PackageInstallOutcome::Installed(_)));
    #[cfg(windows)]
    {
        let PackageInstallOutcome::AwaitingDurability { operation, receipt } = outcome else {
            panic!("existing Windows namespace has no durable receipt")
        };
        assert_eq!(receipt.durability(), Durability::Unconfirmed);
        assert_eq!(operation.stage, OperationStage::AwaitingDurability);
        assert!(
            fixture
                .database
                .plugin_installations()
                .installation(&operation.descriptor.manifest().plugin.id, &operation.descriptor.manifest().plugin.version)
                .await
                .unwrap()
                .is_none()
        );
    }
    fixture.services.shutdown().await;
}

struct BlockedReader {
    source: Cursor<Vec<u8>>,
    entered: Option<oneshot::Sender<()>>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}
impl Read for BlockedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            let (lock, condition) = &*self.gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
        }
        self.source.read(buffer)
    }
}
struct Gate(Arc<(Mutex<bool>, Condvar)>);
impl Drop for Gate {
    fn drop(&mut self) {
        release(self);
    }
}
fn blocked() -> (BlockedReader, oneshot::Receiver<()>, Gate) {
    let (sender, receiver) = oneshot::channel();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    (BlockedReader { source: Cursor::new(data()), entered: Some(sender), gate: Arc::clone(&gate) }, receiver, Gate(gate))
}
fn release(gate: &Gate) {
    let (lock, condition) = &*gate.0;
    *lock.lock().unwrap() = true;
    condition.notify_all();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiter_cancellation_retains_admission_and_shutdown_drains_actual_blocking_work() {
    let fixture = Fixture::new(true).await;
    let (source, entered, first_gate) = blocked();
    let service = fixture.services.packages.clone();
    let actor = fixture.admin.clone();
    let waiter = tokio::spawn(async move { service.install(&actor, id("first"), source, deadline(), MAX_ARCHIVE_WIRE_BYTES).await });
    entered.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(matches!(
        fixture.services.packages.install(&fixture.admin, id("first"), Cursor::new(data()), deadline(), MAX_ARCHIVE_WIRE_BYTES).await,
        Err(ServiceError::Conflict("package operation already active"))
    ));
    let (source, entered, second_gate) = blocked();
    let service = fixture.services.packages.clone();
    let actor = fixture.admin.clone();
    let second = tokio::spawn(async move { service.install(&actor, id("second"), source, deadline(), MAX_ARCHIVE_WIRE_BYTES).await });
    entered.await.unwrap();
    assert!(matches!(
        fixture.services.packages.install(&fixture.admin, id("third"), Cursor::new(data()), deadline(), MAX_ARCHIVE_WIRE_BYTES).await,
        Err(ServiceError::AdmissionUnavailable { scope: "package operation", reason: rbit_storage::AdmissionReason::Capacity })
    ));
    fixture.services.prepare_shutdown();
    let services = fixture.services.clone();
    let mut shutdown = tokio::spawn(async move { services.shutdown().await });
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut shutdown).await.is_err());
    assert!(matches!(
        fixture.services.packages.install(&fixture.admin, id("closed"), Cursor::new(data()), deadline(), MAX_ARCHIVE_WIRE_BYTES).await,
        Err(ServiceError::AdmissionUnavailable { scope: "package operation", reason: rbit_storage::AdmissionReason::Stopped })
    ));
    release(&first_gate);
    release(&second_gate);
    shutdown.await.unwrap();
    let error = second.await.unwrap().unwrap_err();
    assert_eq!(store_failure(&error).error(), StoreError::Cancelled);
    assert_eq!(fixture.database.plugin_installations().recovery(None).await.unwrap(), Vec::<InstallationOperation>::new());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.database.close().await;
    assert!(matches!(
        fixture
            .services
            .packages
            .install(&fixture.admin, id("after-stores-close"), Cursor::new(data()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
            .await,
        Err(ServiceError::AdmissionUnavailable { scope: "package operation", reason: rbit_storage::AdmissionReason::Stopped })
    ));
    assert!(matches!(
        fixture.services.packages.operation(&fixture.admin, &id("first")).await,
        Err(ServiceError::AdmissionUnavailable { scope: "package operation", reason: rbit_storage::AdmissionReason::Stopped })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn administrator_revocation_during_staging_prevents_journal_and_publication() {
    let fixture = Fixture::new(true).await;
    // Keep another administrator so normal moderation can revoke the actor.
    fixture
        .database
        .accounts()
        .create_user(NewUser {
            name: "another-admin".parse().unwrap(),
            email: "another@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".to_owned(),
            is_admin: true,
        })
        .await
        .unwrap();
    let (source, entered, gate) = blocked();
    let service = fixture.services.packages.clone();
    let actor = fixture.admin.clone();
    let worker = tokio::spawn(async move { service.install(&actor, id("revoked"), source, deadline(), MAX_ARCHIVE_WIRE_BYTES).await });
    entered.await.unwrap();
    fixture.database.admin().set_admin(fixture.admin.id(), false).await.unwrap();
    release(&gate);
    assert!(matches!(worker.await.unwrap(), Err(ServiceError::Forbidden)));
    assert!(fixture.database.plugin_installations().operation(&id("revoked")).await.unwrap().is_none());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    assert!(matches!(fixture.services.packages.operation(&fixture.admin, &id("revoked")).await, Err(ServiceError::Forbidden)));
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn concurrent_different_operation_tokens_cannot_repin_the_same_version() {
    let fixture = Fixture::new(true).await;
    let first = data();
    let second = archive(&json!({"type":"data"}), b"other verified bytes", "1.0.0");
    let (left, right) = tokio::join!(
        fixture.services.packages.install(&fixture.admin, id("race-first"), Cursor::new(first), deadline(), MAX_ARCHIVE_WIRE_BYTES),
        fixture.services.packages.install(&fixture.admin, id("race-second"), Cursor::new(second), deadline(), MAX_ARCHIVE_WIRE_BYTES),
    );
    let winner = match (left, right) {
        (Ok(PackageInstallOutcome::Installed(winner)), Err(ServiceError::Conflict(_)))
        | (Err(ServiceError::Conflict(_)), Ok(PackageInstallOutcome::Installed(winner))) => winner,
        outcomes => panic!("unexpected concurrent outcomes: {outcomes:?}"),
    };
    assert_eq!(fixture.database.plugin_installations().objects(None).await.unwrap().as_slice(), std::slice::from_ref(&winner.descriptor));
    assert_eq!(
        fixture
            .database
            .plugin_installations()
            .installation(&winner.descriptor.manifest().plugin.id, &winner.descriptor.manifest().plugin.version)
            .await
            .unwrap(),
        Some(winner)
    );
    // An immutable object published before the losing pin conflict is retained
    // for host reconciliation. There must be no private staging remnants.
    for entry in fs::read_dir(&fixture.root).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        assert!(entry.file_name().to_str().unwrap().ends_with(".rbit-plugin"));
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn failed_publication_can_retry_the_same_token_after_explicit_host_repair() {
    let fixture = Fixture::new(true).await;
    let bytes = data();
    let path = fixture.root.join(format!("{}.rbit-plugin", hash(&bytes)));
    fs::write(&path, b"corrupt existing object").unwrap();
    let token = id("repairable");
    let error = fixture
        .services
        .packages
        .install(&fixture.admin, token.clone(), Cursor::new(bytes.clone()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap_err();
    assert_eq!(store_failure(&error).error(), StoreError::CorruptExisting);
    let failed = fixture.services.packages.operation(&fixture.admin, &token).await.unwrap().unwrap();
    assert!(matches!(failed.stage, OperationStage::Error(_)));
    assert_eq!(failed.revision, 1);
    assert_eq!(fs::read(&path).unwrap(), b"corrupt existing object");
    // Only the host performs repair. The installation service never overwrites
    // or deletes the bad target when reporting its original publication failure.
    fs::remove_file(&path).unwrap();
    let result = fixture
        .services
        .packages
        .install(&fixture.admin, token.clone(), Cursor::new(bytes.clone()), deadline(), MAX_ARCHIVE_WIRE_BYTES)
        .await
        .unwrap();
    assert!(matches!(result, PackageInstallOutcome::Installed(_)));
    let complete = fixture.services.packages.operation(&fixture.admin, &token).await.unwrap().unwrap();
    assert_eq!(complete.stage, OperationStage::Complete);
    assert_eq!(complete.revision, 3);
    assert_eq!(complete.created_at, failed.created_at);
    assert_eq!(complete.actor, failed.actor);
    assert_eq!(fs::read(path).unwrap(), bytes);
    fixture.services.shutdown().await;
}
