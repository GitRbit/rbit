use std::{str::FromStr, time::Duration};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusRecorder};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use super::*;

async fn pool(dir: &tempfile::TempDir, connections: u32, busy_timeout: Duration) -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite:")
        .unwrap()
        .filename(dir.path().join("pool.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(busy_timeout);
    let pool = SqlitePoolOptions::new().max_connections(connections).connect_with(options).await.unwrap();
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER NOT NULL)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO t (id, n) VALUES (1, 0)").execute(&pool).await.unwrap();
    pool
}

fn recorder() -> PrometheusRecorder {
    PrometheusBuilder::new().build_recorder()
}

/// The sample count of `metric` with exactly `labels` in the rendered output.
fn value(text: &str, metric: &str, labels: &str) -> Option<f64> {
    let prefix = format!("{metric}{{{labels}}} ");
    text.lines().find_map(|line| line.strip_prefix(&prefix)).map(|v| v.parse().unwrap())
}

#[tokio::test]
async fn statements_and_transactions_are_timed() {
    let recorder = recorder();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let dir = tempfile::tempdir().unwrap();
    let pool = Pool::new(pool(&dir, 2, Duration::from_secs(5)).await, "things");

    let n: i64 = sqlx::query_scalar("SELECT n FROM t WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
    let rows = sqlx::query("SELECT n FROM t").fetch_all(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);

    let mut tx = pool.begin_immediate().await.unwrap();
    sqlx::query("UPDATE t SET n = n + 1").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let tx = pool.begin().await.unwrap();
    drop(tx);

    let text = recorder.handle().render();
    assert_eq!(value(&text, "rbit_sqlite_acquire_wait_seconds_count", r#"mode="query""#), Some(2.0), "{text}");
    assert_eq!(value(&text, "rbit_sqlite_acquire_wait_seconds_count", r#"mode="transaction""#), Some(2.0), "{text}");
    assert_eq!(value(&text, "rbit_sqlite_write_lock_wait_seconds_count", r#"store="things""#), Some(1.0), "{text}");
    let committed = r#"store="things",outcome="commit""#;
    assert_eq!(value(&text, "rbit_sqlite_write_transaction_seconds_count", committed), Some(1.0), "{text}");
    let dropped = r#"store="things",outcome="rollback""#;
    assert_eq!(value(&text, "rbit_sqlite_write_transaction_seconds_count", dropped), Some(1.0), "{text}");
    assert!(!text.contains("rbit_sqlite_busy_errors_total"), "{text}");
}

/// A deferred transaction that read before another writer committed cannot upgrade:
/// `SQLITE_BUSY_SNAPSHOT`, at once and whatever the busy timeout.
#[tokio::test]
async fn busy_snapshot_is_counted_by_store_and_phase() {
    let recorder = recorder();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let dir = tempfile::tempdir().unwrap();
    let raw = pool(&dir, 2, Duration::from_secs(5)).await;
    let pool = Pool::new(raw.clone(), "things");

    let mut tx = pool.begin().await.unwrap();
    let _: i64 = sqlx::query_scalar("SELECT n FROM t").fetch_one(&mut *tx).await.unwrap();
    sqlx::query("UPDATE t SET n = 5").execute(&raw).await.unwrap();
    let error = sqlx::query("UPDATE t SET n = n + 1").execute(&mut *tx).await.unwrap_err();
    assert_eq!(busy_code(&error), Some("busy_snapshot"), "{error}");
    drop(tx);

    let text = recorder.handle().render();
    let labels = r#"store="things",phase="transaction",code="busy_snapshot""#;
    assert_eq!(value(&text, "rbit_sqlite_busy_errors_total", labels), Some(1.0), "{text}");
}

/// With the write lock held elsewhere past the busy timeout, `BEGIN IMMEDIATE` and a
/// plain write both fail with `SQLITE_BUSY`, each counted where it surfaced.
#[tokio::test]
async fn busy_is_counted_at_begin_and_query() {
    let recorder = recorder();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let dir = tempfile::tempdir().unwrap();
    let raw = pool(&dir, 3, Duration::from_millis(20)).await;
    let pool = Pool::new(raw.clone(), "things");

    let mut holder = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE t SET n = 1").execute(&mut *holder).await.unwrap();
    let error = pool.begin_immediate().await.unwrap_err();
    assert_eq!(busy_code(&error), Some("busy"), "{error}");
    let error = sqlx::query("UPDATE t SET n = 2").execute(&pool).await.unwrap_err();
    assert_eq!(busy_code(&error), Some("busy"), "{error}");
    holder.rollback().await.unwrap();

    let text = recorder.handle().render();
    assert_eq!(value(&text, "rbit_sqlite_busy_errors_total", r#"store="things",phase="begin",code="busy""#), Some(1.0), "{text}");
    assert_eq!(value(&text, "rbit_sqlite_busy_errors_total", r#"store="things",phase="query",code="busy""#), Some(1.0), "{text}");
}

#[tokio::test]
async fn work_is_counted_per_future() {
    let dir = tempfile::tempdir().unwrap();
    let pool = Pool::new(pool(&dir, 2, Duration::from_secs(5)).await, "things");
    let (inner, outer) = crate::count_work(async {
        let _: i64 = sqlx::query_scalar("SELECT n FROM t WHERE id = 1").fetch_one(&pool).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("SELECT n FROM t").fetch_all(&mut conn).await.unwrap();
        sqlx::query("SELECT n FROM t").fetch_optional(&mut conn).await.unwrap();
        drop(conn);
        let mut tx = pool.begin_immediate().await.unwrap();
        sqlx::query("UPDATE t SET n = n + 1").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        crate::count_work(sqlx::query("SELECT 1").execute(&pool)).await.1
    })
    .await;
    assert_eq!(inner, crate::Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(outer, crate::Work { acquisitions: 3, statements: 4, cache_writes: 0 }, "nested work counts once, in the inner scope");
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();
}

#[tokio::test]
async fn acquire_timeouts_are_counted() {
    let recorder = recorder();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::from_str("sqlite:").unwrap().filename(dir.path().join("one.db")).create_if_missing(true);
    // Long enough for the first connection to open on a loaded machine.
    let raw = SqlitePoolOptions::new().max_connections(1).acquire_timeout(Duration::from_millis(500)).connect_with(options).await.unwrap();
    let pool = Pool::new(raw, "things");

    let held = pool.acquire().await.unwrap();
    assert!(matches!(pool.begin().await.unwrap_err(), Error::PoolTimedOut));
    drop(held);

    let text = recorder.handle().render();
    assert_eq!(value(&text, "rbit_sqlite_acquire_timeouts_total", r#"mode="transaction""#), Some(1.0), "{text}");
}

#[test]
fn only_busy_and_locked_codes_are_labelled() {
    let database = |code: i32| Error::Database(Box::new(FakeError(code)));
    assert_eq!(busy_code(&database(5)), Some("busy"));
    assert_eq!(busy_code(&database(517)), Some("busy_snapshot"));
    assert_eq!(busy_code(&database(773)), Some("busy_timeout"));
    assert_eq!(busy_code(&database(261)), Some("busy_recovery"));
    assert_eq!(busy_code(&database(6)), Some("locked"));
    assert_eq!(busy_code(&database(262)), Some("locked"));
    assert_eq!(busy_code(&database(2067)), None, "unique violations are not contention");
    assert_eq!(busy_code(&Error::RowNotFound), None);
}

#[derive(Debug)]
struct FakeError(i32);

impl fmt::Display for FakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "code {}", self.0)
    }
}

impl std::error::Error for FakeError {}

impl sqlx::error::DatabaseError for FakeError {
    fn message(&self) -> &'static str {
        "fake"
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some(self.0.to_string().into())
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }
}
