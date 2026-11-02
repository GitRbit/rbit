//! Fixed outer transaction tests; no lost acknowledgement/physical shutdown proof.
use super::{OuterBeginError, Pool, require_no_tracked_transaction};
use metrics_exporter_prometheus::PrometheusBuilder;
use sqlx::{
    Connection as _, Error,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(2);

async fn fixture(dir: &tempfile::TempDir, connections: u32) -> Pool {
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("outer.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(50));
    let raw = SqlitePoolOptions::new().max_connections(connections).connect_with(options).await.unwrap();
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER NOT NULL)").execute(&raw).await.unwrap();
    sqlx::query("INSERT INTO t VALUES (1, 0)").execute(&raw).await.unwrap();
    Pool::new(raw, "outer_test")
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn acknowledged_outer_commit_and_rollback_preserve_instrumented_executor() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 2).await;
    let ((), work) = crate::count_work(async {
        let mut tx = pool.begin_outer_deferred().await.unwrap();
        assert!(tx.conn.parts().0.is_in_transaction());
        sqlx::query("UPDATE t SET n = 1 WHERE id = 1").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin_outer_deferred().await.unwrap();
        sqlx::query("UPDATE t SET n = 2 WHERE id = 1").execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();
    })
    .await;
    assert_eq!(work, crate::Work { acquisitions: 2, statements: 2, cache_writes: 0 });
    let n: i64 = sqlx::query_scalar("SELECT n FROM t WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn deferred_begin_succeeds_while_other_writer_held_without_immediate_metric() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 2).await;
    let mut writer = pool.begin_immediate().await.unwrap();
    sqlx::query("UPDATE t SET n = 3 WHERE id = 1").execute(&mut *writer).await.unwrap();
    let tx = timeout(WAIT, pool.begin_outer_deferred()).await.unwrap().unwrap();
    tx.commit().await.unwrap();
    writer.rollback().await.unwrap();
    let text = recorder.handle().render();
    let prefix = "rbit_sqlite_write_lock_wait_seconds_count{store=\"outer_test\"} ";
    let count = text.lines().find_map(|line| line.strip_prefix(prefix)).unwrap();
    assert_eq!(count.parse::<u64>().unwrap(), 1, "only unchanged immediate begin records lock wait: {text}");
    assert!(text.contains("mode=\"transaction\""));
}

#[tokio::test]
async fn precheck_rejects_tracked_parent_without_queuing_parent_rollback() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 1).await;
    let mut parent = pool.begin().await.unwrap();
    sqlx::query("UPDATE t SET n = 4 WHERE id = 1").execute(&mut *parent).await.unwrap();
    assert!(matches!(require_no_tracked_transaction(parent.conn.parts().0), Err(OuterBeginError::ExistingTransaction)));
    sqlx::query("UPDATE t SET n = n + 1 WHERE id = 1").execute(&mut *parent).await.unwrap();
    parent.commit().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT n FROM t WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 5, "precheck did not construct/drop a nested SQLx Transaction");
}

#[tokio::test]
async fn closed_pool_failure_is_exact_acquisition_stage_without_begin_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 1).await;
    pool.inner.close().await;
    assert!(matches!(pool.begin_outer_deferred().await, Err(OuterBeginError::Acquire(Error::PoolClosed))));
}

#[tokio::test]
async fn untracked_fixture_transaction_begin_error_remains_unconfirmed() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 1).await;
    let mut conn = pool.wait(super::super::Mode::Transaction).await.unwrap();
    // Explicit unsupported fixture state: raw SQL does NOT update SQLx's tracked depth.
    // It exercises the actual worker-side fixed BEGIN rejection, not a fake error.
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    assert!(!conn.is_in_transaction());
    let result = pool.begin_outer_deferred_acquired(conn).await;
    assert!(matches!(result, Err(OuterBeginError::BeginUnconfirmed(Error::Database(_)))));
    // Test-fixture cleanup only. We do not claim the failed begin's Drop acknowledged
    // cleanup: depth-zero queued rollback is a no-op in this unsupported raw state.
    timeout(WAIT, sqlx::query("ROLLBACK").execute(&pool)).await.unwrap().unwrap();
    pool.begin_outer_deferred().await.unwrap().rollback().await.unwrap();
}

#[tokio::test]
async fn cancellation_waiting_for_connection_submits_no_helper_begin() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 1).await;
    let held = pool.acquire().await.unwrap();
    let mut begin = Box::pin(pool.begin_outer_deferred());
    pending(begin.as_mut()).await;
    drop(begin); // Only acquire pending; no connection was available for this helper.
    drop(held);
    let mut tx = timeout(WAIT, pool.begin_outer_deferred()).await.unwrap().unwrap();
    sqlx::query("UPDATE t SET n = 6 WHERE id = 1").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT n FROM t").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 6);
}

#[tokio::test]
async fn dropped_successful_transaction_is_fallback_cleanup_not_explicit_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let pool = fixture(&directory, 1).await;
    let mut tx = pool.begin_outer_deferred().await.unwrap();
    sqlx::query("UPDATE t SET n = 7 WHERE id = 1").execute(&mut *tx).await.unwrap();
    drop(tx);
    // An observed next query checks this actual path's rollback effect; it is not an
    // API-issued rollback certificate and says nothing about canceled BEGIN/COMMIT.
    let n: i64 = timeout(WAIT, sqlx::query_scalar("SELECT n FROM t").fetch_one(&pool)).await.unwrap().unwrap();
    assert_eq!(n, 0);
}
