//! One supervisor per consumed catalog, including abandoned opening results.
use super::{FullWikiWriter, WikiCatalogHandle, Writer, retention};
use crate::sqlite::pool::Pool;
use std::sync::{Arc, Mutex};
use tokio::sync::{Semaphore, watch};
use tokio_util::task::TaskTracker;

#[derive(Clone, Copy)]
pub(super) enum Completion {
    Waiting,
    Closed,
    Unproved,
}

pub(super) fn own_pool(
    pool: sqlx::SqlitePool,
    catalog: WikiCatalogHandle,
    state: Arc<retention::NativeState>,
    mut retention: retention::NativeRetention,
) -> FullWikiWriter {
    let (stop, mut stopping) = watch::channel(false);
    let (done, completion) = watch::channel(Completion::Waiting);
    let tasks = TaskTracker::new();
    let writer = FullWikiWriter(Arc::new(Writer {
        commands: Pool::new(pool.clone(), "wiki_mutations"),
        pool: pool.clone(),
        gate: Mutex::new(false),
        admission: Arc::new(Semaphore::new(1)),
        tasks: tasks.clone(),
        catalog: catalog.clone(),
        state: Arc::clone(&state),
        completion,
        shutdown: stop,
    }));
    // No controller reference: last controller drop closes the channel. Admitted
    // tasks keep only resources, and remain counted while this supervisor waits.
    tokio::spawn(async move {
        while !*stopping.borrow_and_update() {
            tokio::select! {
                change = stopping.changed() => { if change.is_err() { break; } },
                () = state.unproved() => break,
            }
        }
        tasks.close();
        // Stop SQLx's after_connect retry loop on an unproved generation; waiting
        // for that admitted task before marking the pool closed would delay drain.
        if state.is_uncertain() {
            pool.close().await;
        }
        tasks.wait().await;
        if !state.is_uncertain() {
            let acquired = tokio::select! {
                acquired = pool.acquire() => acquired,
                () = state.unproved() => Err(sqlx::Error::PoolClosed),
            };
            match acquired {
                Ok(connection) => {
                    let _closed = state.close_physical(connection).await;
                }
                Err(_) => state.fail(),
            }
        }
        pool.close().await;
        drop(pool);
        drop(catalog);
        let completion = if state.accounted() {
            retention.verified();
            Completion::Closed
        } else {
            Completion::Unproved
        };
        drop(retention); // Unproved Drop intentionally keeps exactly one real lease.
        done.send_replace(completion);
    });
    writer
}
