//! Actual loose-object scans, not SQL agreement/durability/readiness fixtures.
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_git::wikis::{CatalogBoundWikiObjects, PrepareWikiPage, WikiObjectError, WikiObjectWriter, WikiObjects};
use rbit_kernel::{AdmissionError, BlockingBudget};
use rbit_wikis::{
    WikiContent, WikiId, WikiPageId, WikiRevisionId, WikiStoreId,
    ownership::WikiCatalogOwnership,
    startup::objects::{MAX_SCAN_OBJECT_BYTES, WikiScanBudget, WikiScanRecord, WikiScannedKind},
};
use std::{
    fs::File,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

struct Catalog {
    file: Arc<File>,
    path: PathBuf,
}
impl WikiCatalogOwnership for Catalog {
    type Error = std::io::Error;
    fn check_bound(&self) -> Result<(), Self::Error> {
        self.file.metadata().map(|_| ())
    }
    fn store_id(&self) -> WikiStoreId {
        identity()
    }
    fn catalog_path(&self) -> &Path {
        &self.path
    }
}
fn identity() -> WikiStoreId {
    WikiStoreId::try_from([1; 16]).unwrap()
}
fn wiki() -> WikiId {
    WikiId::new(1).unwrap()
}
fn request(page: i64, text: &str, parent: Option<WikiRevisionId>, at: i64) -> PrepareWikiPage {
    PrepareWikiPage {
        repository: RepoId(7),
        wiki: wiki(),
        page: WikiPageId::new(page).unwrap(),
        parent,
        content: WikiContent::try_from(text.to_owned()).unwrap(),
        actor: AccountId(1),
        at: Timestamp(at),
    }
}
fn fixture() -> (tempfile::TempDir, WikiObjects) {
    let directory = tempfile::tempdir().unwrap();
    let work = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let objects = WikiObjects::open(&directory.path().join("objects"), identity(), work).unwrap();
    (directory, objects)
}
fn bound(directory: &Path, objects: WikiObjects) -> CatalogBoundWikiObjects<Catalog> {
    let path = directory.join("actual-catalog-owner");
    let file = File::create_new(&path).unwrap();
    file.try_lock().unwrap();
    WikiObjectWriter::open(objects).unwrap().bind_catalog(Catalog { file: Arc::new(file), path }).unwrap()
}
async fn collect(owner: &CatalogBoundWikiObjects<Catalog>, records: u16) -> Result<(Vec<WikiScanRecord>, u64), WikiObjectError> {
    let mut cursor = owner.scan_loose(RepoId(7), wiki()).await?;
    let mut result = Vec::new();
    let mut hashed = 0;
    loop {
        let batch = cursor.next(WikiScanBudget::new(records, MAX_SCAN_OBJECT_BYTES).unwrap()).await?;
        assert!(batch.records().len() <= usize::from(records));
        assert!(batch.bytes_hashed() <= MAX_SCAN_OBJECT_BYTES);
        hashed += u64::from(batch.objects_hashed());
        let (records, next) = batch.into_parts();
        result.extend(records);
        let Some(next) = next else { return Ok((result, hashed)) };
        cursor = next;
    }
}

#[tokio::test]
async fn small_batches_hash_shared_history_objects_once_and_emit_complete_ordered_edge_work() {
    let (directory, objects) = fixture();
    let first = objects.prepare(request(11, "shared", None, 1)).await.unwrap();
    let second = objects.prepare(request(12, "other", Some(first), 2)).await.unwrap();
    objects.prepare(request(11, "shared", Some(second), 3)).await.unwrap();
    let owner = bound(directory.path(), objects);
    let (records, hashed) = collect(&owner, 1).await.unwrap();
    assert_eq!(hashed, 7, "two blobs, two trees and three commits; repeated shared blobs are not rehashed");
    let mut kinds = std::collections::HashMap::new();
    let mut completed = std::collections::HashSet::new();
    for record in &records {
        match record {
            WikiScanRecord::Object { id, kind, .. } => {
                assert_eq!(kinds.insert(*id, *kind), None);
            }
            WikiScanRecord::Complete { id } => {
                assert!(kinds.contains_key(id));
                assert!(completed.insert(*id));
            }
            WikiScanRecord::Edge { source, .. } | WikiScanRecord::Commit { id: source, .. } => assert!(kinds.contains_key(source)),
        }
    }
    assert_eq!(completed.len(), 7);
    for record in records {
        if let WikiScanRecord::Edge { target, kind, .. } = record {
            assert_eq!(kinds.get(&target), Some(&kind));
        }
    }
    assert_eq!(kinds.values().filter(|kind| **kind == WikiScannedKind::Blob).count(), 2);
}

#[tokio::test]
async fn continuation_retains_actual_root_and_nonqueued_wiki_scope_until_destroyed() {
    let (directory, objects) = fixture();
    objects.prepare(request(11, "body", None, 1)).await.unwrap();
    let owner = bound(directory.path(), objects.clone());
    let cursor = owner.scan_loose(RepoId(7), wiki()).await.unwrap();
    assert!(matches!(owner.scan_loose(RepoId(7), wiki()).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    drop(owner);
    assert!(matches!(objects.prepare(request(11, "cannot write", None, 2)).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    drop(cursor);
    objects.prepare(request(11, "released", None, 2)).await.unwrap();
}

#[tokio::test]
async fn packed_or_unknown_object_names_are_refused_without_becoming_complete_scan() {
    for name in ["pack/unknown.pack", "unexpected", "aa/not-an-object"] {
        let (directory, objects) = fixture();
        objects.prepare(request(11, "body", None, 1)).await.unwrap();
        let path = directory.path().join("objects/7/1.git/objects").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"refuse").unwrap();
        let owner = bound(directory.path(), objects);
        assert!(collect(&owner, 2).await.is_err());
    }
}

struct Release(Option<std::sync::mpsc::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[test]
fn cancelled_awaiter_retains_actual_root_while_its_native_scan_is_queued() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).max_blocking_threads(1).enable_all().build().unwrap();
    runtime.block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let work = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
        let objects = WikiObjects::open(&directory.path().join("objects"), identity(), work.clone()).unwrap();
        objects.prepare(request(11, "body", None, 1)).await.unwrap();
        let owner = bound(directory.path(), objects.clone());
        let cursor = owner.scan_loose(RepoId(7), wiki()).await.unwrap();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let release = Release(Some(release));
        let blocker = tokio::task::spawn_blocking(move || {
            entered.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        let caller = tokio::spawn(cursor.next(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap()));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while work.available() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(owner);
        assert!(matches!(WikiObjectWriter::open(objects.clone()), Err(WikiObjectError::Admission(AdmissionError::Busy))));
        drop(release);
        blocker.await.unwrap();
        let released = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match WikiObjectWriter::open(objects.clone()) {
                    Ok(owner) => break owner,
                    Err(WikiObjectError::Admission(AdmissionError::Busy)) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected ownership failure: {error:?}"),
                }
            }
        })
        .await
        .unwrap();
        drop(released);
    });
}
