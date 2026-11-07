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
    #[cfg(test)]
    protocol: super::protocol::SqlProtocol,
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
        Self {
            inner,
            store,
            #[cfg(test)]
            protocol: super::protocol::SqlProtocol::legacy(),
        }
    }
    #[cfg(test)]
    pub(super) fn with_protocol(mut self, protocol: super::protocol::SqlProtocol) -> Self {
        self.protocol = protocol;
        self
    }
    #[cfg(test)]
    pub(crate) fn require_legacy(&self) -> Result<(), crate::StorageError> {
        self.protocol.require_legacy()
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
mod tests;
