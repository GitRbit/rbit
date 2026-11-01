#![cfg(unix)]
#[path = "support/image_fixture.rs"]
mod fixture;
use fixture::*;
use rbit_plugin_packages::{ArchiveError, CleanupStatus, ImageError, MAX_ARCHIVE_WIRE_BYTES, NativeImageStore, PackageStore, StageControl};
use serde_json::json;
use std::{
    fs,
    io::Cursor,
    os::unix::fs::{PermissionsExt, symlink},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
fn object(root: &std::path::Path, bytes: &[u8]) -> std::path::PathBuf {
    root.join(format!("{}.rbit-plugin", digest(bytes)))
}
struct ReapedChild(std::process::Child);
impl Drop for ReapedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn control() -> StageControl {
    StageControl::new(Instant::now() + Duration::from_mins(1), MAX_ARCHIVE_WIRE_BYTES).unwrap()
}
fn assert_clean(failure: &rbit_plugin_packages::ImageFailure, images: &std::path::Path) {
    assert_eq!(failure.cleanup(), CleanupStatus::Complete);
    assert_eq!(children(images), Vec::<std::path::PathBuf>::new());
}

#[test]
fn real_cas_native_image_is_exact_owned_private_nonexecuting_and_independent() {
    let cas_root = private_root();
    let image_root = private_root();
    let bytes = archive(&native());
    let cas = PackageStore::open(cas_root.path()).unwrap();
    cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    let images = NativeImageStore::open(image_root.path()).unwrap();
    let first = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap();
    let second = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap();
    assert_ne!(first.root(), second.root());
    assert_eq!(first.package(), &expected(&bytes));
    assert_eq!(first.wire_bytes(), bytes.len() as u64);
    assert_eq!(fs::read(first.entrypoint()).unwrap(), PROBE);
    assert_eq!(fs::read(first.root().join("cfg/settings")).unwrap(), CONFIG);
    for path in [first.root().to_path_buf(), first.root().join("bin"), first.root().join("cfg")] {
        assert_eq!(fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777, 0o700);
    }
    for path in [first.entrypoint().to_path_buf(), first.root().join("cfg/settings")] {
        assert_eq!(fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777, 0o600);
    }
    first.discard().unwrap();
    assert_eq!(fs::read(second.entrypoint()).unwrap(), PROBE);
    second.discard().unwrap();
    assert_eq!(children(image_root.path()), Vec::<std::path::PathBuf>::new());
    assert_eq!(fs::read(object(cas_root.path(), &bytes)).unwrap(), bytes);
}
#[test]
fn exact_identity_kind_size_and_cas_content_must_match_without_completed_images() {
    let cas_root = private_root();
    let image_root = private_root();
    let bytes = archive(&native());
    let cas = PackageStore::open(cas_root.path()).unwrap();
    cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    let images = NativeImageStore::open(image_root.path()).unwrap();
    for (field, value) in [("version", json!("1.0.0+other")), ("plugin_id", json!("other.materializer"))] {
        let mut reference = serde_json::to_value(expected(&bytes)).unwrap();
        reference[field] = value;
        let changed = serde_json::from_value(reference).unwrap();
        let failure = images.prepare(&cas, &changed, bytes.len() as u64, control()).unwrap_err();
        assert_eq!(failure.error(), ImageError::Identity);
        assert_clean(&failure, image_root.path());
    }
    let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64 - 1, control()).unwrap_err();
    assert_eq!(failure.error(), ImageError::Object);
    assert_clean(&failure, image_root.path());
    for kind in [json!({"type":"data"}), json!({"type":"wasm","entrypoint":"bin/probe"})] {
        let bytes = archive(&kind);
        cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
        let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap_err();
        assert_eq!(failure.error(), ImageError::Kind);
        assert_clean(&failure, image_root.path());
    }
    // Direct hostile CAS fixture, not an install receipt: a valid archive stored
    // under another digest must still fail the complete-stream identity check.
    let other = archive(&native());
    let mut reference = serde_json::to_value(expected(&other)).unwrap();
    reference["archive_digest"] = json!("f".repeat(64));
    let changed = serde_json::from_value(reference).unwrap();
    fs::write(cas_root.path().join(format!("{}.rbit-plugin", "f".repeat(64))), &other).unwrap();
    let failure = images.prepare(&cas, &changed, other.len() as u64, control()).unwrap_err();
    assert_eq!(failure.error(), ImageError::Identity);
    assert_clean(&failure, image_root.path());
}

#[test]
fn early_valid_entrypoint_never_escapes_bad_later_payload_trailer_or_truncation() {
    let cas_root = private_root();
    let image_root = private_root();
    let cas = PackageStore::open(cas_root.path()).unwrap();
    let images = NativeImageStore::open(image_root.path()).unwrap();
    let bytes = archive(&native());
    let mut corrupt = bytes.clone();
    let offset = corrupt.windows(CONFIG.len()).position(|window| window == CONFIG).unwrap();
    corrupt[offset] ^= 1;
    let mut trailing = bytes.clone();
    trailing.push(1);
    let mut padding = bytes.clone();
    padding[offset + CONFIG.len()] = 1;
    for (bad, error) in [
        (corrupt, ArchiveError::Digest),
        (trailing, ArchiveError::TrailingData),
        (padding, ArchiveError::Padding),
        (bytes[..bytes.len() - 1].to_vec(), ArchiveError::Truncated),
    ] {
        // Invalid archives are injected directly under their content digest to
        // exercise materialization's own validation, not claim they were installed.
        fs::write(object(cas_root.path(), &bad), &bad).unwrap();
        let failure = images.prepare(&cas, &expected(&bad), bad.len() as u64, control()).unwrap_err();
        assert_eq!(failure.error(), ImageError::Archive(error));
        assert_clean(&failure, image_root.path());
    }
}
#[test]
fn unsafe_roots_and_cancelled_or_expired_work_fail_before_output() {
    let cas_root = private_root();
    let images_root = private_root();
    let bytes = archive(&native());
    let cas = PackageStore::open(cas_root.path()).unwrap();
    cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    let images = NativeImageStore::open(images_root.path()).unwrap();
    let cancelled = control();
    cancelled.cancel();
    for (control, error) in [
        (cancelled, ImageError::Cancelled),
        (
            StageControl::new(Instant::now().checked_sub(Duration::from_secs(1)).unwrap(), MAX_ARCHIVE_WIRE_BYTES).unwrap(),
            ImageError::Deadline,
        ),
    ] {
        let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control).unwrap_err();
        assert_eq!(failure.error(), error);
        assert_clean(&failure, images_root.path());
    }
    assert_eq!(
        NativeImageStore::open(cas_root.path())
            .unwrap()
            .prepare(&cas, &expected(&bytes), bytes.len() as u64, control())
            .unwrap_err()
            .error(),
        ImageError::Root
    );
    let link = images_root.path().join("link");
    symlink(cas_root.path(), &link).unwrap();
    assert_eq!(NativeImageStore::open(&link).unwrap_err(), ImageError::Root);
    fs::set_permissions(images_root.path(), fs::Permissions::from_mode(0o750)).unwrap();
    assert_eq!(NativeImageStore::open(images_root.path()).unwrap_err(), ImageError::Root);
    fs::set_permissions(images_root.path(), fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn concurrent_same_digest_images_do_not_clobber_or_remove_other_owners() {
    let cas_root = private_root();
    let image_root = private_root();
    let bytes = archive(&native());
    let cas = PackageStore::open(cas_root.path()).unwrap();
    cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    let store = NativeImageStore::open(image_root.path()).unwrap();
    thread::scope(|scope| {
        let (ready, arrivals) = mpsc::channel();
        let mut releases = Vec::new();
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let (release, wait) = mpsc::channel();
            releases.push(release);
            let ready = ready.clone();
            let cas = &cas;
            let store = &store;
            let bytes = &bytes;
            tasks.push(scope.spawn(move || {
                let image = store.prepare(cas, &expected(bytes), bytes.len() as u64, control()).unwrap();
                ready.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(3)).unwrap();
                assert_eq!(fs::read(image.entrypoint()).unwrap(), PROBE);
                image.discard().unwrap();
            }));
        }
        drop(ready);
        for _ in 0..3 {
            arrivals.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        for release in releases {
            release.send(()).unwrap();
        }
        for task in tasks {
            task.join().unwrap();
        }
    });
    assert_eq!(children(image_root.path()), Vec::<std::path::PathBuf>::new());
    assert_eq!(fs::read(object(cas_root.path(), &bytes)).unwrap(), bytes);
}

fn first_payload_header(bytes: &[u8]) -> usize {
    let size = std::str::from_utf8(&bytes[124..136]).unwrap().trim_matches(['\0', ' ']);
    512 + usize::from_str_radix(size, 8).unwrap().next_multiple_of(512)
}
#[test]
fn hostile_raw_paths_types_duplicates_and_missing_files_cannot_publish_an_image() {
    let cas_root = private_root();
    let image_root = private_root();
    let cas = PackageStore::open(cas_root.path()).unwrap();
    let images = NativeImageStore::open(image_root.path()).unwrap();
    let bytes = archive(&native());
    let first = first_payload_header(&bytes);
    let mut path = bytes.clone();
    let mut header = tar::Header::new_ustar();
    header.as_mut_bytes().copy_from_slice(&bytes[first..first + 512]);
    header.as_mut_bytes()[..100].fill(0);
    header.as_mut_bytes()[..9].copy_from_slice(b"../escape");
    header.set_cksum();
    path[first..first + 512].copy_from_slice(header.as_bytes());
    let mut link = bytes.clone();
    header.as_mut_bytes().copy_from_slice(&bytes[first..first + 512]);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_cksum();
    link[first..first + 512].copy_from_slice(header.as_bytes());
    let second = first + 512 + PROBE.len().next_multiple_of(512);
    let mut duplicate = bytes.clone();
    duplicate[second..second + 512].copy_from_slice(&bytes[first..first + 512]);
    let mut missing = bytes.clone();
    missing[second..second + 512].fill(0);
    for (bad, error) in [
        (path, ArchiveError::Path),
        (link, ArchiveError::Header),
        (duplicate, ArchiveError::UnexpectedFile),
        (missing, ArchiveError::MissingFile),
    ] {
        fs::write(object(cas_root.path(), &bad), &bad).unwrap();
        let failure = images.prepare(&cas, &expected(&bad), bad.len() as u64, control()).unwrap_err();
        assert_eq!(failure.error(), ImageError::Archive(error));
        assert_clean(&failure, image_root.path());
    }
}

#[test]
fn symlink_cas_source_is_rejected_without_following_it() {
    let cas_root = private_root();
    let images_root = private_root();
    let outside = private_root();
    let bytes = archive(&native());
    let source = outside.path().join("payload");
    fs::write(&source, &bytes).unwrap();
    symlink(&source, object(cas_root.path(), &bytes)).unwrap();
    let cas = PackageStore::open(cas_root.path()).unwrap();
    let images = NativeImageStore::open(images_root.path()).unwrap();
    let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap_err();
    assert_eq!(failure.error(), ImageError::Object);
    assert_clean(&failure, images_root.path());
    assert_eq!(fs::read(source).unwrap(), bytes);
}
#[test]
#[ignore = "child helper invoked only by the bounded FIFO parent"]
fn fifo_child() {
    let Ok(root) = std::env::var("RBIT_IMAGE_TEST_CAS") else { return };
    let images_root = std::env::var("RBIT_IMAGE_TEST_IMAGES").unwrap();
    let bytes = archive(&native());
    let cas = PackageStore::open(&root).unwrap();
    let images = NativeImageStore::open(&images_root).unwrap();
    let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap_err();
    assert_eq!(failure.error(), ImageError::Object);
    assert_clean(&failure, std::path::Path::new(&images_root));
    fs::write(std::path::Path::new(&root).join(".child-ack"), b"verified").unwrap();
}
#[test]
fn fifo_source_is_rejected_before_a_blocking_open_can_hang() {
    let cas_root = private_root();
    let images_root = private_root();
    let bytes = archive(&native());
    fs::write(cas_root.path().join(".child-ack"), b"pending").unwrap();
    assert!(std::process::Command::new("mkfifo").arg(object(cas_root.path(), &bytes)).status().unwrap().success());
    let mut child = ReapedChild(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fifo_child", "--ignored"])
            .env("RBIT_IMAGE_TEST_CAS", cas_root.path())
            .env("RBIT_IMAGE_TEST_IMAGES", images_root.path())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.0.kill();
            let _ = child.0.wait();
            panic!("FIFO rejection child timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(children(images_root.path()), Vec::<std::path::PathBuf>::new());
    assert_eq!(fs::read(cas_root.path().join(".child-ack")).unwrap(), b"verified");
}

#[test]
#[ignore = "child helper invoked only by the restrictive-umask parent"]
fn restrictive_umask_child() {
    let Ok(root) = std::env::var("RBIT_IMAGE_TEST_CAS") else { return };
    let images_root = std::env::var("RBIT_IMAGE_TEST_IMAGES").unwrap();
    let bytes = archive(&native());
    let cas = PackageStore::open(&root).unwrap();
    let images = NativeImageStore::open(&images_root).unwrap();
    let failure = images.prepare(&cas, &expected(&bytes), bytes.len() as u64, control()).unwrap_err();
    assert_eq!(failure.error(), ImageError::Root);
    assert_clean(&failure, std::path::Path::new(&images_root));
    fs::write(std::path::Path::new(&root).join(".child-ack"), b"verified").unwrap();
}
#[test]
fn restrictive_umask_fails_without_silently_chmod_repairing_output() {
    let cas_root = private_root();
    let images_root = private_root();
    let bytes = archive(&native());
    let cas = PackageStore::open(cas_root.path()).unwrap();
    cas.stage(Cursor::new(&bytes), control()).unwrap().publish().unwrap();
    fs::write(cas_root.path().join(".child-ack"), b"pending").unwrap();
    let mut child = ReapedChild(
        std::process::Command::new("sh")
            .args(["-c", "umask 777; exec \"$1\" --exact \"$2\" --ignored", "rbit-image-umask"])
            .arg(std::env::current_exe().unwrap())
            .arg("restrictive_umask_child")
            .env("RBIT_IMAGE_TEST_CAS", cas_root.path())
            .env("RBIT_IMAGE_TEST_IMAGES", images_root.path())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.0.kill();
            let _ = child.0.wait();
            panic!("umask child timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(children(images_root.path()), Vec::<std::path::PathBuf>::new());
    assert_eq!(fs::read(cas_root.path().join(".child-ack")).unwrap(), b"verified");
}
