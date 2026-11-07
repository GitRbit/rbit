use super::super::{IdentityUpgradeLimits, StoppedIdentityUpgrade};
use crate::{Database, KvStore, NewUser, SessionRecord, sqlite::writer_compatibility::register};
use rbit_core::{AccountId, Timestamp};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
};
pub(super) const RECORDS: redb::TableDefinition<'_, &[u8; 32], &[u8]> = redb::TableDefinition::new("sessions_v2");
pub(super) const INDEX: redb::TableDefinition<'_, (i64, &[u8; 32]), ()> = redb::TableDefinition::new("account_sessions");
pub(super) const EPOCHS: redb::TableDefinition<'_, &[u8; 32], &[u8; 32]> = redb::TableDefinition::new("session_epochs_v1");
pub(super) const LEGACY: redb::TableDefinition<'_, &[u8; 32], &[u8; 16]> = redb::TableDefinition::new("sessions");
pub(super) fn limits() -> IdentityUpgradeLimits {
    IdentityUpgradeLimits { max_record_bytes: NonZeroU32::new(65536).unwrap(), max_entries: NonZeroU64::new(4096).unwrap() }
}
pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub sql: PathBuf,
    pub kv: PathBuf,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let sql = directory.path().join("identity.db");
        let kv = directory.path().join("identity.redb");
        let database = Database::open(&sql).await.unwrap();
        for name in ["alice", "bob"] {
            database
                .accounts()
                .create_user(NewUser {
                    name: name.parse().unwrap(),
                    email: format!("{name}@example.com").parse().unwrap(),
                    display_name: None,
                    password_hash: "unchanged PHC".into(),
                    is_admin: false,
                })
                .await
                .unwrap();
        }
        database.close().await;
        {
            let store = KvStore::open(&kv).unwrap();
            let now = Timestamp::now();
            store.sessions().put([1; 32], SessionRecord::at(AccountId(1), Timestamp(now.0 + 3600), None, "fixture", now)).await.unwrap();
        }
        Self { directory, sql, kv }
    }
    pub async fn main_snapshot() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let sql = directory.path().join("main.db");
        let kv = directory.path().join("main.redb");
        let options = register(
            SqliteConnectOptions::new()
                .filename(&sql)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        );
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::migrate!("./migrations").run_to(59, &mut connection).await.unwrap();
        sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','main-user',1,1)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO user_logins(account_id,email,password_hash) VALUES(1,'main@example.com','unchanged PHC')")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        {
            let store = KvStore::open(&kv).unwrap();
            let now = Timestamp::now();
            store.sessions().put([1; 32], SessionRecord::at(AccountId(1), Timestamp(now.0 + 3600), None, "main", now)).await.unwrap();
        }
        Self { directory, sql, kv }
    }
    pub async fn migration_rows(&self) -> Vec<(i64, bool, Vec<u8>)> {
        sqlx::query_as("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut self.connection().await)
            .await
            .unwrap()
    }
    pub async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&register(
            SqliteConnectOptions::new().filename(&self.sql).foreign_keys(true).pragma("trusted_schema", "OFF"),
        ))
        .await
        .unwrap()
    }
    pub async fn owner(&self) -> StoppedIdentityUpgrade {
        StoppedIdentityUpgrade::open(&self.sql, &self.kv, limits()).await.unwrap()
    }
    pub async fn state(&self) -> String {
        sqlx::query_scalar("SELECT mode FROM credential_protocol").fetch_one(&mut self.connection().await).await.unwrap()
    }
    pub fn orphan_epoch(&self, epoch: i64) {
        let db = redb::Database::open(&self.kv).unwrap();
        let tx = db.begin_write().unwrap();
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&999_i64.to_be_bytes());
        bytes[8..16].copy_from_slice(&epoch.to_be_bytes());
        bytes[16..].copy_from_slice(&[7; 16]);
        tx.open_table(EPOCHS).unwrap().insert(&[9; 32], &bytes).unwrap();
        tx.open_table(INDEX).unwrap().insert((999, &[8; 32]), ()).unwrap();
        let mut legacy = [0; 16];
        legacy[..8].copy_from_slice(&1_i64.to_be_bytes());
        tx.open_table(LEGACY).unwrap().insert(&[6; 32], &legacy).unwrap();
        tx.commit().unwrap();
    }
    pub async fn hashes_and_epochs(&self) -> Vec<(String, i64)> {
        sqlx::query_as("SELECT password_hash,authentication_epoch FROM user_logins ORDER BY account_id")
            .fetch_all(&mut self.connection().await)
            .await
            .unwrap()
    }
}
