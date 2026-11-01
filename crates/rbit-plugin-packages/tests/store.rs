use std::{
    fmt::Write as _,
    fs,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use rbit_plugin_packages::{
    CleanupStatus, Durability, MAX_ARCHIVE_WIRE_BYTES, PackageStore, PublicationKind, PublicationVisibility, StageControl, StoreError,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}
fn archive() -> Vec<u8> {
    let payload = b"immutable package payload";
    let manifest = serde_json::to_vec(&json!({
        "schema_version":1,
        "plugin":{"id":"con.example", "version":"1.0.0+host", "host_api":"^1"},
        "kind":{"type":"data"},
        "files":[{"path":"data/file", "size":payload.len(), "sha256":digest(payload)}],
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    }))
    .unwrap();
    let mut bytes = Vec::new();
    for (path, body) in [("package.json", manifest.as_slice()), ("data/file", payload.as_slice())] {
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
fn private_root() -> tempfile::TempDir {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
    }
    #[cfg(not(unix))]
    tempfile::tempdir().unwrap()
}

fn control() -> StageControl {
    StageControl::new(Instant::now() + Duration::from_mins(1), MAX_ARCHIVE_WIRE_BYTES).unwrap()
}
fn children(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root).unwrap().map(|entry| entry.unwrap().path()).collect()
}
fn target(root: &Path, bytes: &[u8]) -> PathBuf {
    root.join(format!("{}.rbit-plugin", digest(bytes)))
}
fn assert_existing_durability(durability: Durability) {
    #[cfg(unix)]
    assert_eq!(durability, Durability::Confirmed);
    #[cfg(windows)]
    assert_eq!(durability, Durability::Unconfirmed);
}

#[test]
fn publishes_the_same_verified_bytes_after_the_original_source_changes() {
    let root = private_root();
    let input = tempfile::tempdir().unwrap();
    let bytes = archive();
    let source = input.path().join("source.tar");
    fs::write(&source, &bytes).unwrap();
    let store = PackageStore::open(root.path()).unwrap();
    let prepared = store.stage(fs::File::open(&source).unwrap(), control()).unwrap();
    assert_eq!(prepared.digest().as_str(), digest(&bytes));
    assert_eq!(children(root.path()).len(), 1);
    assert!(!target(root.path(), &bytes).exists());
    fs::write(&source, b"replacement bytes").unwrap();
    let receipt = prepared.publish().unwrap();
    assert_eq!(receipt.publication(), PublicationKind::Created);
    assert_eq!(receipt.durability(), Durability::Confirmed);
    assert_eq!(receipt.wire_bytes(), bytes.len() as u64);
    assert_eq!(receipt.payload_bytes(), b"immutable package payload".len() as u64);
    assert_eq!(fs::read(target(root.path(), &bytes)).unwrap(), bytes);
    assert_eq!(children(root.path()), [target(root.path(), &bytes)]);
}

#[test]
fn existing_objects_are_rehashed_and_never_replaced() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bytes = archive();
    // A prior host/reconciliation write carries no successful publication
    // receipt. Existing content must be verified and synchronized independently.
    fs::write(target(root.path(), &bytes), &bytes).unwrap();
    let receipt = store.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    assert_eq!(receipt.publication(), PublicationKind::ExistingVerified);
    assert_existing_durability(receipt.durability());
    let path = target(root.path(), &bytes);
    let mut corrupt = bytes.clone();
    corrupt[1024] ^= 1;
    for bad in [corrupt, b"short".to_vec()] {
        fs::write(&path, &bad).unwrap();
        let failure = store.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap_err();
        assert_eq!(failure.error(), StoreError::CorruptExisting);
        assert_eq!(failure.visibility(), PublicationVisibility::Unconfirmed);
        assert!(failure.receipt().is_none());
        assert_eq!(failure.cleanup(), CleanupStatus::Complete);
        assert_eq!(fs::read(&path).unwrap(), bad);
        assert_eq!(children(root.path()), std::slice::from_ref(&path));
    }
}

#[test]
fn concurrent_publications_create_one_object_and_verify_the_rest() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bytes = archive();
    let barrier = Arc::new(Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let store = store.clone();
            let bytes = bytes.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let prepared = store.stage(Cursor::new(bytes), control()).unwrap();
                barrier.wait();
                prepared.publish().unwrap()
            })
        })
        .collect();
    let receipts: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
    assert_eq!(receipts.iter().filter(|receipt| receipt.publication() == PublicationKind::Created).count(), 1);
    for receipt in receipts {
        if receipt.publication() == PublicationKind::ExistingVerified {
            assert_existing_durability(receipt.durability());
        } else {
            assert_eq!(receipt.durability(), Durability::Confirmed);
        }
    }
    assert_eq!(fs::read(target(root.path(), &bytes)).unwrap(), bytes);
    assert_eq!(children(root.path()), [target(root.path(), &bytes)]);
}

#[test]
fn rejects_directory_and_symlink_targets_without_touching_them() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bytes = archive();
    let path = target(root.path(), &bytes);
    fs::create_dir(&path).unwrap();
    let failure = store.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap_err();
    assert_eq!(failure.error(), StoreError::CorruptExisting);
    assert!(path.is_dir());
    fs::remove_dir(&path).unwrap();
    #[cfg(unix)]
    {
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), &bytes).unwrap();
        std::os::unix::fs::symlink(outside.path(), &path).unwrap();
        let failure = store.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap_err();
        assert_eq!(failure.error(), StoreError::CorruptExisting);
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(outside.path()).unwrap(), bytes);
    }
}

#[test]
fn invalid_archives_and_wire_quota_failures_remove_private_staging() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let mut bytes = archive();
    bytes.push(1);
    let failure = store.stage(Cursor::new(bytes), control()).unwrap_err();
    assert!(matches!(failure.error(), StoreError::Archive(_)));
    assert_eq!(failure.visibility(), PublicationVisibility::NotAttempted);
    assert_eq!(failure.cleanup(), CleanupStatus::Complete);
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
    let bytes = archive();
    // The wire quota includes room to attempt the final EOF probe.
    let bounds = StageControl::new(Instant::now() + Duration::from_mins(1), bytes.len() as u64).unwrap();
    let failure = store.stage(Cursor::new(&bytes), bounds).unwrap_err();
    assert_eq!(failure.error(), StoreError::WireLimit);
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
    let bounds = StageControl::new(Instant::now() + Duration::from_mins(1), bytes.len() as u64 + 1).unwrap();
    store.stage(Cursor::new(&bytes), bounds).unwrap().discard().unwrap();
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
}

struct CancellingReader {
    bytes: Cursor<Vec<u8>>,
    control: StageControl,
}
impl Read for CancellingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.bytes.read(buffer)?;
        self.control.cancel();
        Ok(count)
    }
}
struct BrokenReader;
impl Read for BrokenReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("sensitive source detail"))
    }
}

#[test]
fn cancellation_deadline_and_source_errors_cleanup_and_have_static_messages() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let cancelled = control();
    let reader = CancellingReader { bytes: Cursor::new(archive()), control: cancelled.clone() };
    let failure = store.stage(reader, cancelled).unwrap_err();
    assert_eq!(failure.error(), StoreError::Cancelled);
    assert_eq!(failure.cleanup(), CleanupStatus::Complete);
    let expired = StageControl::new(Instant::now().checked_sub(Duration::from_secs(1)).unwrap(), MAX_ARCHIVE_WIRE_BYTES).unwrap();
    assert_eq!(store.stage(Cursor::new(archive()), expired).unwrap_err().error(), StoreError::Deadline);
    let failure = store.stage(BrokenReader, control()).unwrap_err();
    assert_eq!(failure.error(), StoreError::Read);
    assert!(!failure.to_string().contains("sensitive"));
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
}

#[test]
fn cancellation_before_publish_and_explicit_discard_do_not_publish() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bounds = control();
    let prepared = store.stage(Cursor::new(archive()), bounds.clone()).unwrap();
    bounds.cancel();
    let failure = prepared.publish().unwrap_err();
    assert_eq!(failure.error(), StoreError::Cancelled);
    assert_eq!(failure.visibility(), PublicationVisibility::NotAttempted);
    assert_eq!(failure.cleanup(), CleanupStatus::Complete);
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
    store.stage(Cursor::new(archive()), control()).unwrap().discard().unwrap();
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
}

#[test]
fn requires_a_preexisting_private_nonsymlink_root() {
    let root = private_root();
    assert_eq!(PackageStore::open(root.path().join("missing")).unwrap_err(), StoreError::Root);
    let file = root.path().join("file");
    fs::write(&file, b"").unwrap();
    assert_eq!(PackageStore::open(&file).unwrap_err(), StoreError::Root);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(root.path(), &alias).unwrap();
        assert_eq!(PackageStore::open(alias).unwrap_err(), StoreError::Root);
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(PackageStore::open(root.path()).unwrap_err(), StoreError::PrivateRoot);
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let store = PackageStore::open(root.path()).unwrap();
        let prepared = store.stage(Cursor::new(archive()), control()).unwrap();
        let stage = children(root.path()).into_iter().find(|path| path.is_dir() && !path.is_symlink()).unwrap();
        assert_eq!(fs::metadata(&stage).unwrap().permissions().mode() & 0o777, 0o700);
        let payload = children(&stage).pop().unwrap();
        assert_eq!(fs::metadata(payload).unwrap().permissions().mode() & 0o777, 0o600);
        prepared.discard().unwrap();
    }
}

#[cfg(unix)]
#[test]
fn cleanup_failure_retains_a_durable_published_object_and_receipt() {
    use std::os::unix::fs::PermissionsExt;

    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bytes = archive();
    let prepared = store.stage(Cursor::new(&bytes), control()).unwrap();
    let stage = children(root.path()).pop().unwrap();
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o500)).unwrap();
    let result = prepared.publish();
    // Restore permissions before asserting so a failed test leaves no private remnant.
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).unwrap();
    let failure = result.unwrap_err();
    assert_eq!(failure.error(), StoreError::Cleanup);
    assert_eq!(failure.cleanup(), CleanupStatus::Pending);
    assert_eq!(failure.visibility(), PublicationVisibility::Verified);
    let receipt = failure.receipt().unwrap();
    assert_eq!(receipt.publication(), PublicationKind::Created);
    assert_eq!(receipt.durability(), Durability::Confirmed);
    assert_eq!(fs::read(target(root.path(), &bytes)).unwrap(), bytes);
    fs::remove_dir_all(stage).unwrap();
    assert_eq!(children(root.path()), [target(root.path(), &bytes)]);
}

#[test]
fn store_root_paths_preserve_native_encoding() {
    #[cfg(target_os = "linux")]
    let component = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"store-\xff".to_vec())
    };
    #[cfg(windows)]
    let component = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[u16::from(b's'), u16::from(b't'), 0xd800])
    };
    #[cfg(not(any(target_os = "linux", windows)))]
    let component = std::ffi::OsString::from("store-🚀");
    let root = private_root();
    let path = root.path().join(component);
    fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let bytes = archive();
    let store = PackageStore::open(&path).unwrap();
    store.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    assert_eq!(fs::read(target(&path, &bytes)).unwrap(), bytes);
}

struct ChunkedReader {
    bytes: Cursor<Vec<u8>>,
    interrupted: bool,
}
impl Read for ChunkedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.interrupted = !self.interrupted;
        if self.interrupted {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let count = buffer.len().min(1);
        self.bytes.read(&mut buffer[..count])
    }
}

#[test]
fn retries_interrupted_fragmented_reads_without_rehashing_or_rewriting_bytes() {
    let root = private_root();
    let store = PackageStore::open(root.path()).unwrap();
    let bytes = archive();
    let reader = ChunkedReader { bytes: Cursor::new(bytes.clone()), interrupted: false };
    let receipt = store.stage(reader, control()).unwrap().publish().unwrap();
    assert_eq!(receipt.digest().as_str(), digest(&bytes));
    assert_eq!(fs::read(target(root.path(), &bytes)).unwrap(), bytes);
}
