use super::Database;

/// A freshly migrated database in a temporary directory (kept alive by the guard).
pub(crate) async fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    (dir, database)
}

#[tokio::test]
async fn optimize_analyses_tables_that_gained_rows() {
    let (_dir, database) = database().await;
    let analyzed = || async {
        sqlx::query_scalar::<_, String>("SELECT tbl FROM sqlite_stat1 WHERE tbl = 'accounts'")
            .fetch_all(&database.pool)
            .await
            .unwrap_or_default()
    };
    assert!(analyzed().await.is_empty(), "empty tables are left alone");
    sqlx::query("INSERT INTO accounts (kind, name, created_at, updated_at) VALUES ('user', 'ada', 0, 0)")
        .execute(&database.pool)
        .await
        .unwrap();
    database.optimize().await.unwrap();
    assert!(!analyzed().await.is_empty(), "the planner now knows the table's size");
}

/// `.cargo/config.toml` builds the bundled `SQLite` with the query planner stability
/// guarantee on, so no statement is re-prepared because a parameter was rebound.
#[tokio::test]
async fn plans_never_depend_on_bound_values() {
    let (_dir, database) = database().await;
    let qpsg: bool = sqlx::query_scalar("SELECT sqlite_compileoption_used('ENABLE_QPSG')").fetch_one(&database.pool).await.unwrap();
    assert!(qpsg, "SQLite was built without SQLITE_ENABLE_QPSG (LIBSQLITE3_FLAGS in .cargo/config.toml)");
}

/// The `detail` lines of `EXPLAIN QUERY PLAN sql`, in order. Parameters stay unbound.
pub(crate) async fn query_plan(database: &Database, sql: &str) -> Vec<String> {
    let rows: Vec<(i64, i64, i64, String)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}"))).fetch_all(&database.pool).await.unwrap();
    rows.into_iter().map(|(_, _, _, detail)| detail).collect()
}

/// Planner statistics an instance could have: what `PRAGMA optimize` records.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Stats {
    /// None at all, as on a new instance before its tables are analysed.
    None,
    /// Every table analysed at one row, as on a new instance with its first account
    /// and repository (and as long as no table has grown enough to be re-analysed).
    Tiny,
    /// Every table analysed at a million rows, ten per value of an index's first
    /// column and one per unique key.
    Large,
}

pub(crate) const STATS: [Stats; 3] = [Stats::None, Stats::Tiny, Stats::Large];

/// The `detail` lines of `EXPLAIN QUERY PLAN sql` with the planner believing `stats`:
/// on one connection of its own, with `sqlite_stat1` written for every table and
/// index (and `sqlite_stat4` emptied), then reloaded with `ANALYZE sqlite_schema`.
pub(crate) async fn query_plan_with(database: &Database, stats: Stats, sql: &str) -> Vec<String> {
    let mut conn = database.pool.acquire().await.unwrap();
    // Creates the statistics tables if this database has none yet.
    sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
    sqlx::query("DELETE FROM sqlite_stat1").execute(&mut *conn).await.unwrap();
    let has_stat4: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'sqlite_stat4')").fetch_one(&mut *conn).await.unwrap();
    if has_stat4 {
        sqlx::query("DELETE FROM sqlite_stat4").execute(&mut *conn).await.unwrap();
    }
    let rows = match stats {
        Stats::None => None,
        Stats::Tiny => Some(1_u64),
        Stats::Large => Some(1_000_000),
    };
    if let Some(rows) = rows {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND sql NOT LIKE 'CREATE VIRTUAL%'",
        )
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        for table in tables {
            let indexes: Vec<(String, bool)> =
                sqlx::query_as("SELECT name, \"unique\" FROM pragma_index_list(?)").bind(&table).fetch_all(&mut *conn).await.unwrap();
            // A table's own row count is recorded only when it has no index.
            let mut entries = if indexes.is_empty() { vec![(None, rows.to_string())] } else { Vec::new() };
            for (index, unique) in indexes {
                let columns: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM pragma_index_info(?)").bind(&index).fetch_one(&mut *conn).await.unwrap();
                let per_column = (1..=columns)
                    .map(|column| if unique && column == columns { 1 } else { (rows / 10_u64.pow(u32::try_from(column).unwrap())).max(1) });
                let stat = std::iter::once(rows).chain(per_column).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
                entries.push((Some(index), stat));
            }
            for (index, stat) in entries {
                sqlx::query("INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?, ?, ?)")
                    .bind(&table)
                    .bind(index)
                    .bind(stat)
                    .execute(&mut *conn)
                    .await
                    .unwrap();
            }
        }
    }
    sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
    let rows: Vec<(i64, i64, i64, String)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}"))).fetch_all(&mut *conn).await.unwrap();
    // Leave no statistics behind for the next test on this pool.
    sqlx::query("DELETE FROM sqlite_stat1").execute(&mut *conn).await.unwrap();
    sqlx::query("ANALYZE sqlite_schema").execute(&mut *conn).await.unwrap();
    rows.into_iter().map(|(_, _, _, detail)| detail).collect()
}

/// Fails unless `sql` reads every table by a key under every [`STATS`]: no plan line
/// is a `SCAN` other than those starting with one of `allowed` (such as a JSON list of
/// ids, `SCAN json_each`).
pub(crate) async fn assert_no_scans(database: &Database, sql: &str, allowed: &[&str]) {
    for stats in STATS {
        let plan = query_plan_with(database, stats, sql).await;
        let scans: Vec<_> =
            plan.iter().filter(|detail| detail.starts_with("SCAN") && !allowed.iter().any(|ok| detail.starts_with(ok))).collect();
        assert!(scans.is_empty(), "{stats:?} statistics: {scans:?} in {plan:#?}");
    }
}

#[tokio::test]
async fn statistics_scenarios_reach_the_planner() {
    let (_dir, database) = database().await;
    // With one-row statistics a join by primary key is cheaper as a scan of the
    // inner table, so the scenarios do change plans.
    let sql = "SELECT f.repo_id FROM repo_forks f JOIN repositories r ON r.id = f.parent_id WHERE f.created_at = 0";
    let tiny = query_plan_with(&database, Stats::Tiny, sql).await;
    let large = query_plan_with(&database, Stats::Large, sql).await;
    assert_ne!(tiny, large, "{tiny:#?}");
}
