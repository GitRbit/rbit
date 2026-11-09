use super::*;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn session_locale_projection_reuses_the_existing_login_row() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let statement = sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {SESSION_USER}"));
    let rows = sqlx::query_as::<_, (i64, i64, i64, String)>(statement).bind(1_i64).bind(false).fetch_all(&pool).await.unwrap();
    let details: Vec<_> = rows.iter().map(|row| row.3.as_str()).collect();
    assert!(details.iter().any(|detail| detail.contains("SEARCH l USING INTEGER PRIMARY KEY")), "{details:?}");
    assert!(!SESSION_USER.contains("user_preferences"));
    assert!(SESSION_USER.contains("CASE COALESCE(l.interface_locale, l.locale)"));
    pool.close().await;
}
