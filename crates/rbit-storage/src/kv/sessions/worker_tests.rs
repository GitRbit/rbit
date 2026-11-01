//! Real retained redb workers; injected gates/faults are not crash or disk-failure proof.
use super::*;
use rbit_authority_fence::{Domain, DrainError};
use redb::ReadableTableMetadata;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    task::Poll,
    time::Duration,
};
use tokio::{
    sync::Notify,
    time::{Instant, timeout},
};

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    _dir: tempfile::TempDir,
    domain: Domain,
    kv: crate::KvStore,
    store: SessionStore,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let domain = Domain::new();
        let kv = crate::KvStore::open_bound(&dir.path().join("kv.redb"), domain.writer()).unwrap();
        let store = kv.sessions();
        Self { _dir: dir, domain, kv, store }
    }
}
struct Gate {
    entered: Notify,
    once: AtomicBool,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Gate {
    fn new() -> (Arc<Self>, mpsc::SyncSender<()>) {
        let (send, receive) = mpsc::sync_channel(1);
        (Arc::new(Self { entered: Notify::new(), once: AtomicBool::new(false), release: Mutex::new(receive) }), send)
    }
    fn wait(&self) -> Result<(), StorageError> {
        if !self.once.swap(true, Ordering::SeqCst) {
            self.entered.notify_one(); // Actual queued/running worker checkpoint.
            self.release
                .lock()
                .map_err(|_| StorageError::Conflict("injected gate poisoned"))?
                .recv_timeout(WAIT)
                .map_err(|_| StorageError::Conflict("injected gate closed"))?;
        }
        Ok(())
    }
}
fn record(account: i64, expires_in: i64) -> SessionRecord {
    SessionRecord::new(AccountId(account), Timestamp(Timestamp::now().seconds() + expires_in), None, "fixture")
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
fn key(id: usize) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&u64::try_from(id).unwrap().to_be_bytes());
    digest
}
fn rows(store: &SessionStore) -> usize {
    let tx = store.db.begin_read().unwrap();
    usize::try_from(tx.open_table(SESSIONS).unwrap().len().unwrap()).unwrap()
}
fn fill(store: &SessionStore, count: usize, expired: impl Fn(usize) -> bool) {
    // Raw bulk fixture only: one real commit avoids hundreds of fixture fsyncs.
    let tx = store.db.begin_write().unwrap();
    {
        let mut sessions = tx.open_table(SESSIONS).unwrap();
        let mut index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
        for id in 0..count {
            let value = record(1, if expired(id) { -3600 } else { 3600 }).encode().unwrap();
            sessions.insert(&key(id), value.as_slice()).unwrap();
            index.insert((1, &key(id)), ()).unwrap();
        }
    }
    tx.commit().unwrap();
}

#[tokio::test]
async fn dropped_waiter_does_not_release_real_worker_mutation_before_commit() {
    let f = Fixture::new();
    let (gate, release) = Gate::new();
    let owned_gate = gate.clone();
    let owned_store = f.store.clone();
    let waiter = tokio::spawn(async move {
        owned_store
            .write(move |db| {
                owned_gate.wait()?;
                put_session(db, &[1; 32], &record(7, 3600))
            })
            .await
    });
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    assert_eq!(f.domain.snapshot().active, 1);
    waiter.abort();
    assert!(timeout(WAIT, waiter).await.unwrap().unwrap_err().is_cancelled());
    assert_eq!(f.domain.snapshot().active, 1);
    let shutdown = f.domain.stop_writers();
    let mut drained = Box::pin(shutdown.drain(Instant::now() + WAIT));
    pending(drained.as_mut()).await;
    release.send(()).unwrap();
    drop(timeout(WAIT, drained).await.unwrap().unwrap());
    assert_eq!(f.store.get([1; 32]).await.unwrap().unwrap().account_id, AccountId(7));
    assert_eq!(f.store.list_for_account(AccountId(7)).await.unwrap().len(), 1);
    assert!(!f.domain.snapshot().uncertain, "actual worker certified its commit despite lost waiter");
}

#[tokio::test]
async fn injected_worker_error_and_panic_are_uncertain_not_clean_settlement() {
    let f = Fixture::new();
    let error: Result<(), _> = f.store.write(|_| Err(StorageError::Conflict("injected worker error"))).await;
    assert!(matches!(error, Err(StorageError::Conflict("injected worker error"))));
    assert!(f.domain.snapshot().uncertain);
    f.store.put([1; 32], record(1, 3600)).await.unwrap();
    assert!(matches!(f.domain.drain(Instant::now() + WAIT).await, Err(DrainError::Uncertain)));
    let other = Fixture::new();
    let panic: Result<(), _> = other.store.write(|_| panic!("injected session worker panic")).await;
    assert!(matches!(panic, Err(StorageError::Join(_))));
    assert!(other.domain.snapshot().uncertain);
}

#[tokio::test]
async fn bounded_admission_precedes_submission_and_queued_cancel_stays_clean() {
    let f = Fixture::new();
    let quiet = f.domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut queued: Vec<_> = (0..rbit_authority_fence::MAX_MUTATIONS).map(|id| Box::pin(f.store.delete(key(id)))).collect();
    for call in &mut queued {
        pending(call.as_mut()).await;
    }
    let submitted = Arc::new(AtomicBool::new(false));
    let marker = submitted.clone();
    let refused = f
        .store
        .write(move |_| {
            marker.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;
    assert!(matches!(refused, Err(StorageError::AdmissionUnavailable { scope: "session", reason: crate::AdmissionReason::Capacity })));
    assert!(!submitted.load(Ordering::SeqCst));
    drop(queued);
    assert_eq!(f.domain.snapshot().queued, 0);
    drop(quiet);
    drop(f.domain.drain(Instant::now() + WAIT).await.unwrap());
    assert_eq!(rows(&f.store), 0);
}

#[tokio::test]
async fn renewal_abort_replacement_lazy_cleanup_and_account_index_remain_consistent() {
    let f = Fixture::new();
    let live = record(1, 3600);
    assert!(!f.store.renew([9; 32], live.expires_at, live.last_seen_at).await.unwrap());
    f.store.put([1; 32], record(1, -3600)).await.unwrap();
    assert!(!f.store.renew([1; 32], live.expires_at, live.last_seen_at).await.unwrap());
    drop(f.domain.drain(Instant::now() + WAIT).await.unwrap());
    let quiet = f.domain.drain(Instant::now() + WAIT).await.unwrap();
    let owned = f.store.clone();
    let cleanup = tokio::spawn(async move { owned.get([1; 32]).await });
    timeout(WAIT, async {
        while f.domain.snapshot().queued != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap(); // get observed expiry and is waiting before cleanup submission.
    // Raw fixture replacement simulates the preexisting cleanup recheck race.
    put_session(&f.store.db, &[1; 32], &record(2, 3600)).unwrap();
    drop(quiet);
    assert!(timeout(WAIT, cleanup).await.unwrap().unwrap().unwrap().is_none());
    assert_eq!(f.store.get([1; 32]).await.unwrap().unwrap().account_id, AccountId(2));
    assert_eq!(f.store.list_for_account(AccountId(1)).await.unwrap(), Vec::<StoredSession>::new());
    let later = Timestamp(live.expires_at.seconds() + 3600);
    assert!(f.store.renew([1; 32], later, live.last_seen_at).await.unwrap());
    assert!(f.store.renew([1; 32], live.expires_at, Timestamp(1)).await.unwrap());
    assert_eq!(f.store.get([1; 32]).await.unwrap().unwrap().expires_at, later);
    f.store.put([2; 32], record(2, 3600)).await.unwrap();
    assert_eq!(f.store.delete_for_account(AccountId(2), Some([2; 32])).await.unwrap(), 1);
    assert!(!f.store.renew([1; 32], later, live.last_seen_at).await.unwrap());
    assert_eq!(f.store.list_for_account(AccountId(2)).await.unwrap()[0].digest, [2; 32]);
    drop(f.domain.drain(Instant::now() + WAIT).await.unwrap());
}

#[tokio::test]
async fn stop_blocks_each_actual_writer_but_live_reads_and_live_only_scan_stay_independent() {
    let f = Fixture::new();
    f.store.put([1; 32], record(1, 3600)).await.unwrap();
    f.store.put([2; 32], record(1, -3600)).await.unwrap();
    f.domain.stop_writers();
    for result in [
        f.store.put([3; 32], record(1, 3600)).await,
        f.store.renew([1; 32], Timestamp(4_000_000_000), Timestamp::now()).await.map(|_| ()),
        f.store.delete([1; 32]).await,
        f.store.delete_for_account(AccountId(1), None).await.map(|_| ()),
        f.store.get([2; 32]).await.map(|_| ()),
        f.store.purge_expired().await.map(|_| ()),
    ] {
        assert!(matches!(result, Err(StorageError::AdmissionUnavailable { scope: "session", reason: crate::AdmissionReason::Stopped })));
    }
    assert!(f.store.get([1; 32]).await.unwrap().is_some());
    assert_eq!(f.store.list_for_account(AccountId(1)).await.unwrap().len(), 1);
    assert!(!f.domain.snapshot().uncertain);
    let live = Fixture::new();
    fill(&live.store, PURGE_BATCH_SIZE * 2 + 1, |_| false);
    let quiet = live.domain.drain(Instant::now() + WAIT).await.unwrap();
    assert_eq!(timeout(WAIT, live.store.purge_expired()).await.unwrap().unwrap(), 0);
    drop(quiet);
}

#[tokio::test]
async fn lost_scan_waiter_retains_one_purge_reservation_without_holding_authority_serial() {
    let f = Fixture::new();
    fill(&f.store, PURGE_BATCH_SIZE + 1, |_| true);
    let (gate, release) = Gate::new();
    let scan_gate = gate.clone();
    let owned = f.store.clone();
    let waiter = tokio::spawn(async move {
        owned
            .purge_with(
                move |state| {
                    scan_gate.wait()?;
                    state.scan()
                },
                PurgeState::flush,
            )
            .await
    });
    timeout(WAIT, gate.entered.notified()).await.unwrap();
    waiter.abort();
    assert!(timeout(WAIT, waiter).await.unwrap().unwrap_err().is_cancelled());
    assert!(matches!(
        f.kv.clone().sessions().purge_expired().await,
        Err(StorageError::AdmissionUnavailable { scope: "session purge", reason: crate::AdmissionReason::Capacity })
    ));
    assert_eq!(f.domain.snapshot().active, 0);
    f.store.put([255; 32], record(2, 3600)).await.unwrap(); // Real writer interleaves while scan is gated.
    release.send(()).unwrap();
    drop(timeout(WAIT, f.store.purge.clone().acquire_owned()).await.unwrap().unwrap());
    assert_eq!(f.store.purge_expired().await.unwrap(), PURGE_BATCH_SIZE + 1);
    assert_eq!(f.store.get([255; 32]).await.unwrap().unwrap().account_id, AccountId(2));
}

#[tokio::test]
async fn sparse_expiry_keeps_one_snapshot_and_original_candidate_batching() {
    let f = Fixture::new();
    fill(&f.store, PURGE_BATCH_SIZE * 3 + 1, |id| id == 0 || id == PURGE_BATCH_SIZE * 3);
    let (gate, release) = Gate::new();
    let scan_gate = gate.clone();
    let writes = Arc::new(AtomicUsize::new(0));
    let counter = writes.clone();
    let owned = f.store.clone();
    let purge = tokio::spawn(async move {
        owned
            .purge_with(
                move |state| {
                    scan_gate.wait()?;
                    state.scan()
                },
                move |state, db| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    state.flush(db)
                },
            )
            .await
    });
    timeout(WAIT, gate.entered.notified()).await.unwrap(); // Original snapshot already captured.
    f.store.put([255; 32], record(2, -3600)).await.unwrap();
    release.send(()).unwrap();
    assert_eq!(timeout(WAIT, purge).await.unwrap().unwrap().unwrap(), 2);
    assert_eq!(writes.load(Ordering::SeqCst), 1, "sparse chunks are accumulated until snapshot end");
    assert_eq!(rows(&f.store), PURGE_BATCH_SIZE * 3);
    assert_eq!(f.store.purge_expired().await.unwrap(), 1, "new record was outside the original snapshot");
}

fn fail_after_committed_scan(state: PurgeState) -> Result<PurgeState, StorageError> {
    if state.removed > 0 { Err(StorageError::Conflict("injected purge scan failure")) } else { state.scan() }
}
fn fail_after_committed_write(state: PurgeState, db: &redb::Database) -> Result<PurgeState, StorageError> {
    if state.removed > 0 { Err(StorageError::Conflict("injected purge write failure")) } else { state.flush(db) }
}

#[tokio::test]
async fn partial_purge_scan_error_is_known_progress_but_actual_write_error_is_uncertain() {
    let scan = Fixture::new();
    fill(&scan.store, PURGE_BATCH_SIZE + 1, |_| true);
    assert!(matches!(
        scan.store.purge_with(fail_after_committed_scan, PurgeState::flush).await,
        Err(StorageError::Conflict("injected purge scan failure"))
    ));
    assert_eq!(rows(&scan.store), 1);
    assert!(!scan.domain.snapshot().uncertain);
    drop(scan.domain.drain(Instant::now() + WAIT).await.unwrap());
    let write = Fixture::new();
    fill(&write.store, PURGE_BATCH_SIZE + 1, |_| true);
    assert!(matches!(
        write.store.purge_with(PurgeState::scan, fail_after_committed_write).await,
        Err(StorageError::Conflict("injected purge write failure"))
    ));
    assert_eq!(rows(&write.store), 1);
    assert!(write.domain.snapshot().uncertain);
    assert!(matches!(write.domain.drain(Instant::now() + WAIT).await, Err(DrainError::Uncertain)));
}

#[tokio::test]
async fn stop_between_acknowledged_batches_is_known_partial_maintenance() {
    let f = Fixture::new();
    fill(&f.store, PURGE_BATCH_SIZE + 1, |_| true);
    let domain = f.domain.clone();
    let result = f
        .store
        .purge_with(
            move |state| {
                if state.removed > 0 {
                    domain.stop_writers();
                }
                state.scan()
            },
            PurgeState::flush,
        )
        .await;
    assert!(matches!(result, Err(StorageError::AdmissionUnavailable { scope: "session", reason: crate::AdmissionReason::Stopped })));
    assert_eq!(rows(&f.store), 1);
    assert!(!f.domain.snapshot().uncertain);
    drop(f.domain.stop_writers().drain(Instant::now() + WAIT).await.unwrap());
}
