//! Native preparation ownership only; fixtures are data, never SQL/auth receipts.
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_git::wikis::{PrepareWikiPage, WikiObjectError, WikiObjectWriter, WikiObjects};
use rbit_kernel::{AdmissionError, BlockingBudget};
use rbit_wikis::{
    WikiContent, WikiGeneration, WikiId, WikiPageId, WikiStoreId,
    commands::{WikiActorIncarnation, WikiCommandId, WikiIntent, WikiWriteCommand},
    policy::WikiExpected,
};
use std::{
    num::NonZeroUsize,
    path::Path,
    time::{Duration, SystemTime},
};

fn store() -> WikiStoreId {
    WikiStoreId::try_from([1; 16]).unwrap()
}
fn objects(path: &Path, capacity: usize) -> WikiObjects {
    WikiObjects::open(path, store(), BlockingBudget::new(NonZeroUsize::new(capacity).unwrap())).unwrap()
}
fn request(text: &str) -> PrepareWikiPage {
    PrepareWikiPage {
        repository: RepoId(7),
        wiki: WikiId::new(1).unwrap(),
        page: WikiPageId::new(11).unwrap(),
        parent: None,
        content: WikiContent::try_from(text.to_owned()).unwrap(),
        actor: AccountId(1),
        at: Timestamp(123),
    }
}
fn intent(wiki: i64) -> WikiIntent {
    WikiIntent {
        store: store(),
        actor_incarnation: WikiActorIncarnation::new(1).unwrap(),
        command: WikiWriteCommand {
            id: WikiCommandId::new([1; 16]).unwrap(),
            actor: AccountId(1),
            expected: WikiExpected::Create { repository: RepoId(7), wiki: WikiId::new(wiki).unwrap(), head: None },
            path: "Guide".parse().unwrap(),
            title: "Private title".parse().unwrap(),
            content: WikiContent::try_from("Private body".to_owned()).unwrap(),
        },
        page: WikiPageId::new(11).unwrap(),
        generation: WikiGeneration::INITIAL,
        accepted_at: Timestamp(123),
        target: None,
    }
}
fn loose(root: &Path, id: gix::ObjectId) -> std::path::PathBuf {
    let hex = id.to_string();
    root.join("7/1.git/objects").join(&hex[..2]).join(&hex[2..])
}
fn writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(windows)]
    {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}

#[tokio::test]
async fn equal_existing_blob_tree_and_commit_are_reused_without_rewriting_files() {
    let directory = tempfile::tempdir().unwrap();
    let objects = objects(directory.path(), 1);
    let revision = objects.prepare(request("original")).await.unwrap();
    let raw = gix::open_opts(directory.path().join("7/1.git"), gix::open::Options::isolated()).unwrap();
    let commit = raw.find_commit(gix::ObjectId::try_from(revision.as_bytes()).unwrap()).unwrap();
    let tree = commit.tree_id().unwrap().detach();
    let blob = gix::objs::compute_hash(raw.object_hash(), gix::objs::Kind::Blob, b"original").unwrap();
    let paths = [loose(directory.path(), commit.id), loose(directory.path(), tree), loose(directory.path(), blob)];
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(946_684_800);
    for path in &paths {
        writable(path);
        std::fs::OpenOptions::new().write(true).open(path).unwrap().set_modified(old).unwrap();
    }
    assert_eq!(objects.prepare(request("original")).await.unwrap(), revision);
    for path in paths {
        assert_eq!(std::fs::metadata(path).unwrap().modified().unwrap(), old);
    }
}

#[tokio::test]
async fn mismatched_existing_loose_bytes_are_refused_without_overwriting_the_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let objects = objects(directory.path(), 1);
    objects.prepare(request("AAAA original")).await.unwrap();
    let raw = gix::open_opts(directory.path().join("7/1.git"), gix::open::Options::isolated()).unwrap();
    let expected = gix::objs::compute_hash(raw.object_hash(), gix::objs::Kind::Blob, b"AAAA original").unwrap();
    let wrong = raw.write_blob("BBBB original").unwrap().detach();
    let wrong_kind = raw
        .write_object(gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry { mode: gix::objs::tree::EntryKind::Blob.into(), filename: "11.md".into(), oid: wrong }],
        })
        .unwrap()
        .detach();
    let path = loose(directory.path(), expected);
    for replacement in [wrong, wrong_kind] {
        writable(&path);
        std::fs::copy(loose(directory.path(), replacement), &path).unwrap();
        let evidence = std::fs::read(&path).unwrap();
        assert!(objects.prepare(request("AAAA original")).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), evidence);
    }
}

#[tokio::test]
async fn managed_clones_share_nonqueued_wiki_and_prepared_capacity_until_values_drop() {
    let directory = tempfile::tempdir().unwrap();
    let objects = objects(directory.path(), 2);
    let writer = WikiObjectWriter::open(objects.clone()).unwrap();
    let clone = writer.clone();
    assert!(matches!(WikiObjectWriter::open(objects.clone()), Err(WikiObjectError::Admission(AdmissionError::Busy))));
    assert!(matches!(objects.prepare(request("unmanaged")).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    let first = writer.prepare(intent(1)).await.unwrap();
    assert!(matches!(clone.prepare(intent(1)).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    let second = clone.prepare(intent(2)).await.unwrap();
    assert!(matches!(writer.prepare(intent(3)).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    drop(first);
    let retried = clone.prepare(intent(1)).await.unwrap();
    assert_eq!(retried.intent().actor_incarnation.get(), 1);
    drop(writer);
    drop(clone);
    assert!(matches!(objects.prepare(request("unmanaged")).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    drop(second);
    drop(retried);
    objects.prepare(request("unmanaged")).await.unwrap();
}

#[test]
fn cancelled_queued_preparation_retains_the_real_root_owner_until_worker_cleanup() {
    use std::{future::Future, task::Poll};
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).max_blocking_threads(1).enable_all().build().unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        started.send(()).unwrap();
        resume.recv().unwrap();
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let objects = objects(directory.path(), 1);
    let writer = WikiObjectWriter::open(objects.clone()).unwrap();
    runtime.block_on(async {
        let mut pending = Box::pin(writer.prepare(intent(1)));
        std::future::poll_fn(|cx| match pending.as_mut().poll(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("blocked worker cannot finish"),
        })
        .await;
        drop(pending);
        assert!(!directory.path().join("7/1.git").exists());
    });
    drop(writer);
    assert!(matches!(WikiObjectWriter::open(objects.clone()), Err(WikiObjectError::Admission(AdmissionError::Busy))));
    release.send(()).unwrap();
    runtime.block_on(async {
        blocker.await.unwrap();
        let reopened = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match WikiObjectWriter::open(objects.clone()) {
                    Ok(owner) => break owner,
                    Err(WikiObjectError::Admission(AdmissionError::Busy)) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected lock failure: {error:?}"),
                }
            }
        })
        .await
        .unwrap();
        drop(reopened);
    });
    assert!(directory.path().join("7/1.git").exists());
}
