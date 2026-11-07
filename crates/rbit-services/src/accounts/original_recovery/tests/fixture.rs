use crate::{ServiceError, Services};
use rbit_core::{AccountId, Config, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{
    Database, EpochSnapshot, KvStore, NewUser, SessionRecord,
    sqlite::{credential_epochs::allocate_in, credential_revocations::RevocationIntent, writer_compatibility::register},
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::sync::Arc;
pub(super) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub config: Config,
    pub database: Database,
    pub kv: KvStore,
    pub raw: SqlitePool,
    pub services: Services,
    pub account: AccountId,
    pub original: CredentialEpoch,
    pub current: EpochSnapshot,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().to_path_buf();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let sessions = kv.sessions().epoch_sidecar();
        sessions.initialize_identity_if_empty().await.unwrap();
        database.credential_revocations().initialize_pair(&sessions).await.unwrap();
        let user = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "old PHC".into(),
                is_admin: false,
            })
            .await
            .unwrap();
        let raw = SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(config.storage.sqlite_path()).foreign_keys(true)))
            .await
            .unwrap();
        let mut tx = raw.begin().await.unwrap();
        let original = allocate_in(&mut tx).await.unwrap();
        sqlx::query("UPDATE user_logins SET authentication_epoch=?").bind(original.get()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let services = Services::new(Arc::new(config.clone()), &database, &kv).await.unwrap();
        let now = Timestamp::now();
        let record = SessionRecord::at(user.id(), Timestamp(now.0 + 3600), None, "fixture", now);
        let current = sessions.put_bound([1; 32], record.clone(), user.id(), original).await.unwrap().unwrap();
        sessions.put_bound([2; 32], record, user.id(), original).await.unwrap();
        Self { _directory: directory, config, database, kv, raw, services, account: user.id(), original, current }
    }
    pub async fn change(&self) -> RevocationIntent {
        self.database
            .profiles()
            .set_password_hash_if_current_preserving(
                self.account,
                "new PHC",
                "old PHC",
                self.original,
                Timestamp::now(),
                self.current.promotion_spec().unwrap(),
            )
            .await
            .unwrap()
            .unwrap()
    }
    pub async fn outcome(&self, id: i64) -> Option<String> {
        sqlx::query_scalar("SELECT outcome FROM credential_revocations WHERE id=?").bind(id).fetch_one(&self.raw).await.unwrap()
    }
    pub async fn execute(&self, sql: &'static str) {
        sqlx::raw_sql(sql).execute(&self.raw).await.unwrap();
    }
    pub async fn recover(&self, intent: RevocationIntent) -> Result<rbit_identity::sessions::SessionRecoveryDisposition, ServiceError> {
        self.services.accounts.recover_original_session_intent(intent).await
    }
}
