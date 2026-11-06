use super::*;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn barrier_query_plans_use_exact_account_epoch_unique_lookup_not_history_scan() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for (statement, login) in [(LOGIN_SNAPSHOT, true), (FENCED_SESSION_USER, false)] {
        // Statement comes only from the two fixed production query constants above.
        let sql = sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {statement}"));
        let query = sqlx::query_as::<_, (i64, i64, i64, String)>(sql);
        let rows = if login {
            query.bind("alice").fetch_all(&pool).await.unwrap()
        } else {
            query.bind(1_i64).bind(false).bind(0_i64).fetch_all(&pool).await.unwrap()
        };
        let details: Vec<_> = rows.iter().map(|row| row.3.as_str()).collect();
        assert!(
            details.iter().any(|detail| detail.contains("SEARCH r USING INDEX sqlite_autoindex_credential_revocations_1")
                && detail.contains("account_id=? AND target_epoch=?")),
            "{details:?}"
        );
        assert!(!details.iter().any(|detail| detail.starts_with("SCAN r")), "{details:?}");
        if !login {
            assert!(details.iter().any(|detail| detail.contains("SEARCH l USING INTEGER PRIMARY KEY")), "{details:?}");
            assert!(!FENCED_SESSION_USER.contains("user_preferences"));
            assert!(FENCED_SESSION_USER.contains("CASE l.locale"));
        }
    }
}
