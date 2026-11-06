//! Dormant allocator/legacy-seed proof against temporary SQLite and redb stores.
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::sqlite::{
    credential_epochs::{EpochAllocationError, allocate_in, raise_floor_in},
    credential_revocations as intents,
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

mod allocation;
mod seed;

async fn legacy() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new()).in_memory(true).foreign_keys(true))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run_to(60, &pool).await.unwrap();
    seed_users(&pool).await;
    pool
}
async fn seed_users(pool: &SqlitePool) {
    sqlx::raw_sql("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','alice',10,10),(2,'user','bob',10,10); \
        INSERT INTO user_logins(account_id,email,password_hash,is_admin) VALUES(1,'alice@example.com','fixture hash',1),(2,'bob@example.com','fixture hash',0);")
        .execute(pool).await.unwrap();
}
async fn upgrade(pool: &SqlitePool) {
    sqlx::migrate!("./migrations").run(pool).await.unwrap();
}
async fn high_water(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator WHERE singleton=1").fetch_one(pool).await.unwrap()
}
async fn allocate(pool: &SqlitePool) -> CredentialEpoch {
    let mut tx = pool.begin().await.unwrap();
    let value = allocate_in(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    value
}
