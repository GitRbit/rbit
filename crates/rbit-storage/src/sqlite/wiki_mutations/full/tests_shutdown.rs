//! Deterministic dropped-result ownership, without sleeps or an unbounded reaper.
use super::*;
use std::time::Duration;

#[tokio::test]
async fn cancelled_setup_future_keeps_cleanup_owner_until_observed_physical_close() {
    let (_directory, database) = crate::sqlite::plans::database().await;
    let lease = database.wiki_catalog_lease().await.unwrap();
    let (ready, initialized) = tokio::sync::oneshot::channel();
    let (_resume, waiting) = tokio::sync::oneshot::channel::<()>();
    let setup = tokio::spawn(opening::open_after(lease, async move {
        ready.send(()).unwrap();
        let _ = waiting.await;
    }));
    initialized.await.unwrap();
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    setup.abort();
    assert!(setup.await.unwrap_err().is_cancelled());
    let lease = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match database.wiki_catalog_lease().await {
                Ok(lease) => break lease,
                Err(WikiCatalogLeaseError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected catalog error: {error:?}"),
            }
        }
    })
    .await
    .unwrap();
    drop(lease);
}

#[tokio::test]
async fn discarded_opening_result_retains_catalog_until_real_pool_close_completes() {
    let (_directory, database) = crate::sqlite::plans::database().await;
    let lease = database.wiki_catalog_lease().await.unwrap();
    let (ready, opened) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let opening = tokio::spawn(async move {
        let writer = FullWikiWriter::open(lease).await.unwrap();
        ready.send((writer.0.pool.clone(), writer.0.tasks.clone())).unwrap();
        resume.await.unwrap();
        writer
    });
    let (pool, tasks) = opened.await.unwrap();
    let borrowed = pool.acquire().await.unwrap();
    drop(opening); // Its successful eventual result is dropped by Tokio.
    release.send(()).unwrap();
    // The counted scope closes before the physical-connection close waits for this
    // deliberately borrowed connection. No test holds a controller reference.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !tasks.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(borrowed);
    let lease = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match database.wiki_catalog_lease().await {
                Ok(lease) => break lease,
                Err(WikiCatalogLeaseError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected catalog error: {error:?}"),
            }
        }
    })
    .await
    .unwrap();
    drop(lease);
}
