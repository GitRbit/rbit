//! Real pool-derived catalog exclusion spans different roots and native cancellation.
use super::{
    acceptance::{capacity, command},
    fixture::Fixture,
};
use rbit_core::Timestamp;
use rbit_kernel::BlockingBudget;
use rbit_services::wikis::{WikiStorageOpenError, WikiStorageOwner};
use rbit_storage::{Database, sqlite::wiki_mutations::ownership::WikiCatalogLeaseError};
use std::{num::NonZeroUsize, time::Duration};

fn budget() -> BlockingBudget {
    BlockingBudget::new(NonZeroUsize::new(1).unwrap())
}

#[tokio::test]
async fn separate_catalog_compositions_with_different_roots_cannot_bypass_actual_database_lease() {
    let fixture = Fixture::new().await;
    let first_root = fixture.directory.path().join("managed-a");
    let second_root = fixture.directory.path().join("managed-b");
    let first = WikiStorageOwner::open(&fixture.database, &first_root, budget()).await.unwrap();
    let clone = first.clone();
    let separate_pool = Database::open(&fixture.directory.path().join("wiki.db")).await.unwrap();
    assert!(matches!(
        WikiStorageOwner::open(&separate_pool, &second_root, budget()).await,
        Err(WikiStorageOpenError::Catalog(WikiCatalogLeaseError::Busy))
    ));
    assert!(!second_root.exists(), "catalog exclusion precedes any second-root initialization");
    let intent = first.catalog().accept(command(&fixture, 1, "private"), Timestamp(1), capacity()).await.unwrap();
    let prepared = first.prepare(intent).await.unwrap();
    drop(first);
    drop(clone);
    assert!(matches!(
        WikiStorageOwner::open(&separate_pool, &second_root, budget()).await,
        Err(WikiStorageOpenError::Catalog(WikiCatalogLeaseError::Busy))
    ));
    assert!(!second_root.exists());
    drop(prepared);
    WikiStorageOwner::open(&separate_pool, &second_root, budget()).await.unwrap();
    assert!(second_root.exists());
}

#[test]
fn cancelled_queued_native_work_keeps_catalog_lease_until_real_worker_result_destruction() {
    use std::{future::Future, task::Poll};
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).max_blocking_threads(1).enable_all().build().unwrap();
    let fixture = runtime.block_on(Fixture::new());
    let root = fixture.directory.path().join("managed-a");
    let owner = runtime.block_on(WikiStorageOwner::open(&fixture.database, &root, budget())).unwrap();
    let intent = runtime.block_on(owner.catalog().accept(command(&fixture, 1, "private"), Timestamp(1), capacity())).unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        started.send(()).unwrap();
        resume.recv().unwrap();
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    runtime.block_on(async {
        let mut queued = Box::pin(owner.prepare(intent));
        std::future::poll_fn(|cx| match queued.as_mut().poll(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("worker is blocked"),
        })
        .await;
        drop(queued);
    });
    drop(owner);
    assert!(matches!(runtime.block_on(fixture.database.wiki_catalog_lease()), Err(WikiCatalogLeaseError::Busy)));
    release.send(()).unwrap();
    runtime.block_on(async {
        blocker.await.unwrap();
        let lease = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match fixture.database.wiki_catalog_lease().await {
                    Ok(lease) => break lease,
                    Err(WikiCatalogLeaseError::Busy) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected ownership error: {error:?}"),
                }
            }
        })
        .await
        .unwrap();
        drop(lease);
    });
    assert!(root.join(format!("{}/{}.git", fixture.repo.repo.repo.id.0, fixture.wiki.get())).exists());
}
