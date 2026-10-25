//! The connection pool every store shares, instrumented for lock contention.
//!
//! [`Pool`] wraps the one `SqlitePool` with the name of the store using it, and is an
//! [`Executor`] itself, so stores run `query.fetch_one(&self.pool)` exactly as before.
//! Transactions come from [`Pool::begin`] (deferred) and [`Pool::begin_immediate`]
//! (`BEGIN IMMEDIATE`, which takes the write lock up front through the busy handler).
//!
//! Recorded through the [`metrics`] facade (a no-op until a recorder is installed):
//!
//! * `rbit_sqlite_acquire_wait_seconds{mode}` — waiting for a pooled connection, for a
//!   single statement (`mode="query"`) or a transaction (`mode="transaction"`).
//! * `rbit_sqlite_acquire_timeouts_total{mode}` — acquisitions that gave up.
//! * `rbit_sqlite_write_lock_wait_seconds{store}` — `BEGIN IMMEDIATE`, i.e. waiting for
//!   the database's single write lock.
//! * `rbit_sqlite_write_transaction_seconds{store, outcome}` — from `BEGIN` to the end
//!   of the transaction, `outcome` being `commit`, `rollback` (explicit or dropped) or
//!   `failed` (the `COMMIT` itself failed).
//! * `rbit_sqlite_busy_errors_total{store, phase, code}` — `SQLITE_BUSY` and
//!   `SQLITE_LOCKED` errors by extended code (`busy`, `busy_snapshot`, `busy_timeout`,
//!   `busy_recovery`, `locked`) and where they surfaced: `begin`, `query` (a statement
//!   outside a transaction), `transaction` (a statement inside one) or `commit`.
//!
//! Labels are static strings, so the number of series is bounded by the stores. The
//! cost per statement is two clock reads and one histogram update.
//!
//! Every acquisition and statement is also counted for [`crate::count_work`].

use std::{fmt, time::Instant};

use crate::work;
use futures_util::{
    FutureExt, StreamExt, TryFutureExt, TryStreamExt,
    future::BoxFuture,
    stream::{self, BoxStream},
};
use sqlx::{
    Describe, Either, Error, Execute, Executor, SqlStr, Sqlite, SqliteConnection, Transaction,
    pool::PoolConnection,
    sqlite::{SqlitePool, SqliteQueryResult, SqliteRow, SqliteStatement, SqliteTypeInfo},
};

/// The shared pool, labelled with the store that uses it.
#[derive(Clone)]
pub(crate) struct Pool {
    inner: SqlitePool,
    store: &'static str,
}

impl fmt::Debug for Pool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pool").field("store", &self.store).finish_non_exhaustive()
    }
}

/// What a connection is acquired for.
#[derive(Clone, Copy)]
enum Mode {
    Query,
    Transaction,
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Transaction => "transaction",
        }
    }
}

impl Pool {
    pub(crate) fn new(inner: SqlitePool, store: &'static str) -> Self {
        Self { inner, store }
    }

    /// A connection for several statements outside a transaction.
    pub(crate) async fn acquire(&self) -> Result<Conn, Error> {
        Ok(Conn { held: Held::Pooled(self.wait(Mode::Query).await?), store: self.store })
    }

    /// A deferred transaction: it reads from a snapshot and takes the write lock at its
    /// first write. A transaction that reads before it writes cannot wait for the lock
    /// there (`SQLite` skips the busy handler once a read is open): it fails at once with
    /// `SQLITE_BUSY` while another writer holds the lock, or `SQLITE_BUSY_SNAPSHOT` if
    /// one committed since the read. Use [`Self::begin_immediate`] for those.
    pub(crate) async fn begin(&self) -> Result<WriteTransaction, Error> {
        self.start(None).await
    }

    /// `BEGIN IMMEDIATE`: waits (through the busy handler) for the write lock first.
    pub(crate) async fn begin_immediate(&self) -> Result<WriteTransaction, Error> {
        self.start(Some("BEGIN IMMEDIATE")).await
    }

    async fn start(&self, statement: Option<&'static str>) -> Result<WriteTransaction, Error> {
        let conn = self.wait(Mode::Transaction).await?;
        let started = Instant::now();
        // `Transaction::begin` is what `SqlitePool::begin` uses; the public alternatives
        // all borrow the connection, and the store's transaction must own it.
        let tx = Transaction::begin(conn, statement.map(SqlStr::from_static)).await.inspect_err(|e| busy(self.store, "begin", e))?;
        if statement.is_some() {
            metrics::histogram!("rbit_sqlite_write_lock_wait_seconds", "store" => self.store).record(started.elapsed().as_secs_f64());
        }
        Ok(WriteTransaction {
            conn: Conn { held: Held::Transaction(tx), store: self.store },
            timer: Timer { store: self.store, started, outcome: "rollback" },
        })
    }

    async fn wait(&self, mode: Mode) -> Result<PoolConnection<Sqlite>, Error> {
        work::acquisition();
        let started = Instant::now();
        match self.inner.acquire().await {
            Ok(conn) => {
                metrics::histogram!("rbit_sqlite_acquire_wait_seconds", "mode" => mode.label()).record(started.elapsed().as_secs_f64());
                Ok(conn)
            }
            Err(error) => {
                if matches!(error, Error::PoolTimedOut) {
                    metrics::counter!("rbit_sqlite_acquire_timeouts_total", "mode" => mode.label()).increment(1);
                }
                Err(error)
            }
        }
    }
}

impl<'p> Executor<'p> for &'_ Pool {
    type Database = Sqlite;

    fn execute<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<SqliteQueryResult, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let pool = self.clone();
        async move {
            let mut conn = pool.wait(Mode::Query).await?;
            work::statement();
            (&mut *conn).execute(query).await.inspect_err(|e| busy(pool.store, "query", e))
        }
        .boxed()
    }

    fn fetch_all<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Vec<SqliteRow>, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let pool = self.clone();
        async move {
            let mut conn = pool.wait(Mode::Query).await?;
            work::statement();
            (&mut *conn).fetch_all(query).await.inspect_err(|e| busy(pool.store, "query", e))
        }
        .boxed()
    }

    /// Collects the results before yielding them: no store streams rows, and a stream
    /// borrowing its own pooled connection cannot be written without a generator.
    fn fetch_many<'e, 'q: 'e, E>(self, query: E) -> BoxStream<'e, Result<Either<SqliteQueryResult, SqliteRow>, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let pool = self.clone();
        stream::once(async move {
            let mut conn = pool.wait(Mode::Query).await?;
            work::statement();
            (&mut *conn).fetch_many(query).try_collect::<Vec<_>>().await.inspect_err(|e| busy(pool.store, "query", e))
        })
        .map_ok(|steps| stream::iter(steps.into_iter().map(Ok)))
        .try_flatten()
        .boxed()
    }

    fn fetch_optional<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Option<SqliteRow>, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let pool = self.clone();
        async move {
            let mut conn = pool.wait(Mode::Query).await?;
            work::statement();
            (&mut *conn).fetch_optional(query).await.inspect_err(|e| busy(pool.store, "query", e))
        }
        .boxed()
    }

    fn prepare_with<'e>(self, sql: SqlStr, parameters: &'e [SqliteTypeInfo]) -> BoxFuture<'e, Result<SqliteStatement, Error>>
    where
        'p: 'e,
    {
        let pool = self.inner.clone();
        async move { (&mut *pool.acquire().await?).prepare_with(sql, parameters).await }.boxed()
    }

    fn describe<'e>(self, sql: SqlStr) -> BoxFuture<'e, Result<Describe<Sqlite>, Error>>
    where
        'p: 'e,
    {
        let pool = self.inner.clone();
        async move { (&mut *pool.acquire().await?).describe(sql).await }.boxed()
    }
}

/// A connection held for several statements: pooled, or inside a transaction.
pub(crate) struct Conn {
    held: Held,
    store: &'static str,
}

enum Held {
    Pooled(PoolConnection<Sqlite>),
    Transaction(Transaction<'static, Sqlite>),
}

impl Conn {
    fn parts(&mut self) -> (&mut SqliteConnection, &'static str, &'static str) {
        match &mut self.held {
            Held::Pooled(conn) => (conn, self.store, "query"),
            Held::Transaction(tx) => (tx, self.store, "transaction"),
        }
    }
}

impl fmt::Debug for Conn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let transaction = matches!(self.held, Held::Transaction(_));
        f.debug_struct("Conn").field("store", &self.store).field("transaction", &transaction).finish_non_exhaustive()
    }
}

impl<'c> Executor<'c> for &'c mut Conn {
    type Database = Sqlite;

    fn execute<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<SqliteQueryResult, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (conn, store, phase) = self.parts();
        work::statement();
        conn.execute(query).inspect_err(move |e| busy(store, phase, e)).boxed()
    }

    fn fetch_all<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Vec<SqliteRow>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (conn, store, phase) = self.parts();
        work::statement();
        conn.fetch_all(query).inspect_err(move |e| busy(store, phase, e)).boxed()
    }

    fn fetch_many<'e, 'q: 'e, E>(self, query: E) -> BoxStream<'e, Result<Either<SqliteQueryResult, SqliteRow>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (conn, store, phase) = self.parts();
        work::statement();
        conn.fetch_many(query).inspect_err(move |e| busy(store, phase, e)).boxed()
    }

    fn fetch_optional<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Option<SqliteRow>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (conn, store, phase) = self.parts();
        work::statement();
        conn.fetch_optional(query).inspect_err(move |e| busy(store, phase, e)).boxed()
    }

    fn prepare_with<'e>(self, sql: SqlStr, parameters: &'e [SqliteTypeInfo]) -> BoxFuture<'e, Result<SqliteStatement, Error>>
    where
        'c: 'e,
    {
        self.parts().0.prepare_with(sql, parameters)
    }

    fn describe<'e>(self, sql: SqlStr) -> BoxFuture<'e, Result<Describe<Sqlite>, Error>>
    where
        'c: 'e,
    {
        self.parts().0.describe(sql)
    }
}

/// A transaction from [`Pool::begin`] or [`Pool::begin_immediate`]. Run statements with
/// `&mut *tx`; dropping it without [`Self::commit`] rolls it back.
#[derive(Debug)]
pub(crate) struct WriteTransaction {
    conn: Conn,
    timer: Timer,
}

impl WriteTransaction {
    pub(crate) async fn commit(self) -> Result<(), Error> {
        let Self { conn, mut timer } = self;
        let Held::Transaction(tx) = conn.held else { return Ok(()) };
        let result = tx.commit().await.inspect_err(|e| busy(conn.store, "commit", e));
        timer.outcome = if result.is_ok() { "commit" } else { "failed" };
        result
    }

    pub(crate) async fn rollback(self) -> Result<(), Error> {
        let Self { conn, timer: _timer } = self;
        let Held::Transaction(tx) = conn.held else { return Ok(()) };
        tx.rollback().await
    }
}

impl std::ops::Deref for WriteTransaction {
    type Target = Conn;

    fn deref(&self) -> &Conn {
        &self.conn
    }
}

impl std::ops::DerefMut for WriteTransaction {
    fn deref_mut(&mut self) -> &mut Conn {
        &mut self.conn
    }
}

/// Records the transaction's duration when it ends, however it ends.
#[derive(Debug)]
struct Timer {
    store: &'static str,
    started: Instant,
    outcome: &'static str,
}

impl Drop for Timer {
    fn drop(&mut self) {
        metrics::histogram!("rbit_sqlite_write_transaction_seconds", "store" => self.store, "outcome" => self.outcome)
            .record(self.started.elapsed().as_secs_f64());
    }
}

/// Counts `error` if it is `SQLITE_BUSY` or `SQLITE_LOCKED` (in any extended form).
fn busy(store: &'static str, phase: &'static str, error: &Error) {
    if let Some(code) = busy_code(error) {
        metrics::counter!("rbit_sqlite_busy_errors_total", "store" => store, "phase" => phase, "code" => code).increment(1);
    }
}

/// The label for a busy or locked error's extended result code.
fn busy_code(error: &Error) -> Option<&'static str> {
    let Error::Database(error) = error else { return None };
    let code: i32 = error.code()?.parse().ok()?;
    match code {
        // SQLITE_BUSY_RECOVERY, SQLITE_BUSY_SNAPSHOT, SQLITE_BUSY_TIMEOUT.
        261 => Some("busy_recovery"),
        517 => Some("busy_snapshot"),
        773 => Some("busy_timeout"),
        _ if code & 0xff == 5 => Some("busy"),
        _ if code & 0xff == 6 => Some("locked"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
        let raw =
            SqlitePoolOptions::new().max_connections(1).acquire_timeout(Duration::from_millis(500)).connect_with(options).await.unwrap();
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
}
