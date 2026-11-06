//! Inactive credential-fence transaction and upgrade acceptance.
use rbit_storage::sqlite::credential_revocations::RevocationIntent;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[path = "credential_revocations/schema.rs"]
mod schema;
#[path = "credential_revocations/transactions.rs"]
mod transactions;

async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new().in_memory(true)).foreign_keys(true))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    seed(&pool).await;
    pool
}
async fn seed(pool: &SqlitePool) {
    sqlx::raw_sql(
        "INSERT INTO accounts (id,kind,name,created_at,updated_at) VALUES (1,'user','alice',10,10),(2,'user','bob',10,10); \
         INSERT INTO user_logins(account_id,email,password_hash,is_admin) VALUES (1,'alice@example.com','old hash',1),(2,'bob@example.com','bob hash',0); \
         INSERT INTO email_tokens(digest,account_id,purpose,email,created_at,expires_at) VALUES (zeroblob(32),1,'reset','alice@example.com',10,100);"
    ).execute(pool).await.unwrap();
}
async fn state(pool: &SqlitePool) -> (String, i64, Option<i64>, i64) {
    sqlx::query_as(
        "SELECT password_hash, authentication_epoch, (SELECT used_at FROM email_tokens LIMIT 1), \
         (SELECT count(*) FROM credential_revocations) FROM user_logins WHERE account_id=1",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}
async fn stored(pool: &SqlitePool, intent: RevocationIntent) -> (Option<String>, Option<i64>) {
    sqlx::query_as("SELECT outcome, completed_at FROM credential_revocations WHERE id=?").bind(intent.id).fetch_one(pool).await.unwrap()
}
