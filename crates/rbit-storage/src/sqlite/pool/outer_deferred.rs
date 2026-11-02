//! Fixed outer deferred transactions for the concrete repository command phases.
//! Legacy begin/immediate paths remain unchanged; attempted-BEGIN errors stay unconfirmed.
use sqlx::{Connection as _, Error, SqlStr, Sqlite, Transaction, pool::PoolConnection};
use std::time::Instant;

use super::{Conn, Held, Mode, Pool, Timer, WriteTransaction, busy};

#[cfg(test)]
mod tests;

/// Stage evidence, never a blanket driver/rollback settlement certificate.
#[derive(Debug)]
pub(in crate::sqlite) enum OuterBeginError {
    /// No connection obtained; this helper enqueued no BEGIN or DML.
    /// Pool constructor/hooks/earlier commands remain outside this statement.
    Acquire(Error),
    /// Tracked transaction already present; no new Transaction constructed.
    /// Its owner/tail is unknown, not certified clean by this rejection.
    ExistingTransaction,
    /// BEGIN was attempted. Its error or cleanup does not prove settlement.
    /// Retains the exact original error; never classified by strings/codes.
    BeginUnconfirmed(Error),
}

impl Pool {
    /// Fixed OUTER deferred BEGIN. A successful return cannot be a SAVEPOINT.
    /// No new lock-wait histogram: deferred BEGIN does not acquire a writer lock.
    /// Cancellation yields no returned stage evidence and must be owned unknown
    /// by the caller; `SQLx`'s queued cleanup is not an acknowledged rollback.
    pub(in crate::sqlite) async fn begin_outer_deferred(&self) -> Result<WriteTransaction, OuterBeginError> {
        let conn = self.wait(Mode::Transaction).await.map_err(OuterBeginError::Acquire)?;
        self.begin_outer_deferred_acquired(conn).await
    }

    // Concrete owned connection from this pool's instrumented acquisition. This
    // private split keeps pre-acquire and post-acquire provenance distinct; it
    // accepts no arbitrary SQL, executor or callback.
    async fn begin_outer_deferred_acquired(&self, conn: PoolConnection<Sqlite>) -> Result<WriteTransaction, OuterBeginError> {
        require_no_tracked_transaction(&conn)?;
        let started = Instant::now();
        // The precheck avoids constructing SQLx's open=true Transaction on a
        // known nested connection: its error Drop could roll back the parent.
        // The fixed custom BEGIN also checks depth on the actual worker, so a
        // successful acknowledgement cannot silently represent SAVEPOINT.
        let tx = Transaction::begin(conn, Some(SqlStr::from_static("BEGIN")))
            .await
            .inspect_err(|error| busy(self.store, "begin", error))
            .map_err(OuterBeginError::BeginUnconfirmed)?;
        Ok(WriteTransaction {
            conn: Conn { held: Held::Transaction(tx), store: self.store },
            timer: Timer { store: self.store, started, outcome: "rollback" },
        })
    }
}

fn require_no_tracked_transaction(conn: &sqlx::SqliteConnection) -> Result<(), OuterBeginError> {
    if conn.is_in_transaction() { Err(OuterBeginError::ExistingTransaction) } else { Ok(()) }
}
