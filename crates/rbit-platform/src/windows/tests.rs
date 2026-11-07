//! Actual unprivileged temporary filesystem cases; none mints a Wiki head cap.
use super::*;
use crate::{Component, PlatformError};
use std::{fs, io::Write};

fn name(value: &str) -> Component {
    Component::new(value).unwrap()
}

#[test]
fn native_ntfs_write_through_publish_replace_retire_and_parent_exclusion() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("root-文");
    fs::create_dir(&path).unwrap(); // Existing deployment base, not a durable-cap fixture.
    {
        let root = OwnedDirectory::open_local_ntfs(&path).unwrap();
        assert!(matches!(OwnedDirectory::open_local_ntfs(&path), Err(PlatformError::Busy)));
        let objects = root.create_child(&name("objects")).unwrap();
        let fanout = objects.create_child(&name("ab")).unwrap();
        let mut stage = fanout.stage_file().unwrap();
        stage.write_all(b"exact object").unwrap();
        let object = stage.publish_new(&name("123456")).unwrap();
        assert_eq!(fs::read(object.path()).unwrap(), b"exact object");
        let identity = PublishedFile::inspect(&object.path()).unwrap();
        let modified = fs::metadata(object.path()).unwrap().modified().unwrap();
        let mut conflict = fanout.stage_file().unwrap();
        conflict.write_all(b"different bytes").unwrap();
        assert!(matches!(conflict.publish_new(&name("123456")), Err(PlatformError::Conflict)));
        assert_eq!(PublishedFile::inspect(&object.path()).unwrap(), identity);
        assert_eq!(fs::metadata(object.path()).unwrap().modified().unwrap(), modified);
        assert_eq!(fs::read(object.path()).unwrap(), b"exact object");

        let refs = root.create_child(&name("refs")).unwrap();
        let trash = root.create_child(&name("retired")).unwrap();
        let mut first = refs.stage_file().unwrap();
        first.write_all(b"first\n").unwrap();
        let first = first.publish_new(&name("head")).unwrap();
        let mut next = refs.stage_file().unwrap();
        next.write_all(b"second\n").unwrap();
        let next = next.replace_mutable(&name("head")).unwrap();
        assert!(first.check().is_err());
        assert_eq!(fs::read(next.path()).unwrap(), b"second\n");
        let retired = refs.retire_ref(&name("head"), &trash).unwrap();
        assert!(!refs.path().join("head").exists());
        assert_eq!(fs::read(retired.path()).unwrap(), b"second\n");
        retired.check().unwrap();
        assert!(fs::rename(&path, temporary.path().join("renamed")).is_err());
        assert_eq!(format!("{root:?} {object:?}"), "OwnedDirectory(..) PublishedFile(..)");
    }
    fs::rename(&path, temporary.path().join("renamed")).unwrap();
}

#[test]
fn staged_drop_only_removes_its_nonce_and_child_keeps_real_parent_handles() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("root");
    fs::create_dir(&path).unwrap();
    let root = OwnedDirectory::open_local_ntfs(&path).unwrap();
    fs::write(path.join("untouched"), b"evidence").unwrap();
    let mut stage = root.stage_file().unwrap();
    stage.write_all(b"temporary").unwrap();
    assert_eq!(fs::read_dir(&path).unwrap().count(), 2);
    drop(stage);
    assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
    assert_eq!(fs::read(path.join("untouched")).unwrap(), b"evidence");
    let child = root.create_child(&name("parent")).unwrap().create_child(&name("child")).unwrap();
    drop(root);
    assert!(fs::rename(path.join("parent"), path.join("other")).is_err());
    child.check().unwrap();
    drop(child);
    fs::rename(path.join("parent"), path.join("other")).unwrap();
}

#[test]
fn hardlinked_mutable_target_is_refused_and_original_is_untouched() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("root");
    fs::create_dir(&path).unwrap();
    let root = OwnedDirectory::open_local_ntfs(&path).unwrap();
    fs::write(root.path().join("head"), b"old").unwrap();
    fs::hard_link(root.path().join("head"), root.path().join("alias")).unwrap();
    let mut replacement = root.stage_file().unwrap();
    replacement.write_all(b"new").unwrap();
    assert!(matches!(replacement.replace_mutable(&name("head")), Err(PlatformError::Unsupported)));
    assert_eq!(fs::read(root.path().join("head")).unwrap(), b"old");
}

#[test]
fn native_pending_unknown_and_malformed_completion_retain_one_real_subject() {
    for case in ["pending", "unknown", "malformed-success"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("root");
        fs::create_dir(&root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "windows::tests::native_status_child", "--nocapture"])
            .env("RBIT_TEST_PLATFORM_NATIVE_CASE", case)
            .env("RBIT_TEST_PLATFORM_NATIVE_ROOT", &root)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{case}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{case} safety deadline");
            }
            // Safety polling only. Native-lock assertions do not depend on sleep.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        temporary.close().unwrap();
    }
}

#[test]
fn native_status_child() {
    let Ok(case) = std::env::var("RBIT_TEST_PLATFORM_NATIVE_CASE") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("RBIT_TEST_PLATFORM_NATIVE_ROOT").unwrap());
    if case == "remote-refusal" {
        assert!(matches!(OwnedDirectory::open_local_ntfs(&path), Err(PlatformError::Unsupported | PlatformError::Quarantined)));
        return;
    }
    let status = match case.as_str() {
        "pending" => 0x103,
        "unknown" => -1,
        "malformed-success" => 0,
        _ => panic!("unknown model"),
    };
    assert!(matches!(binding::root_with(&path, |subject| native::simulated_status(subject, status)), Err(PlatformError::Quarantined)));
    for _ in 0..3 {
        assert!(matches!(OwnedDirectory::open_local_ntfs(&path), Err(PlatformError::Busy)));
    }
    // Status fault is simulated; retained native file/lease and process cleanup are real.
}

#[test]
#[ignore = "requires an explicit disposable scratch directory on another NTFS volume"]
fn native_cross_volume_move_never_copies_or_deletes_source() {
    let other = std::env::var_os("RBIT_TEST_PLATFORM_SECOND_VOLUME").expect("explicit scratch fixture required");
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir_in(other).unwrap();
    let first_root = first.path().join("root");
    let second_root = second.path().join("root");
    fs::create_dir(&first_root).unwrap();
    fs::create_dir(&second_root).unwrap();
    let first_owner = OwnedDirectory::open_local_ntfs(&first_root).unwrap();
    let second_owner = OwnedDirectory::open_local_ntfs(&second_root).unwrap();
    assert_ne!(first_owner.root.identity.volume, second_owner.root.identity.volume, "fixture needs distinct volumes");
    let source = first.path().join("source");
    let target = second.path().join("target");
    fs::write(&source, b"retained").unwrap();
    assert!(matches!(native::publish_new(&source, &target), Err(PlatformError::Unsupported)));
    assert_eq!(fs::read(source).unwrap(), b"retained");
    assert!(!target.exists());
}

#[test]
#[ignore = "requires an explicit disposable UNC or mapped-share scratch directory"]
fn native_remote_filesystem_is_refused_even_if_server_reports_ntfs() {
    let path = std::env::var_os("RBIT_TEST_PLATFORM_REMOTE_SCRATCH").expect("explicit scratch fixture required");
    let temporary = tempfile::tempdir_in(path).unwrap();
    let root = temporary.path().join("root");
    fs::create_dir(&root).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "windows::tests::native_status_child", "--nocapture"])
        .env("RBIT_TEST_PLATFORM_NATIVE_CASE", "remote-refusal")
        .env("RBIT_TEST_PLATFORM_NATIVE_ROOT", &root)
        .status()
        .unwrap();
    assert!(status.success());
    temporary.close().unwrap();
}
