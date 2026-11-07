use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{SessionPairIdentity, SessionRevocationRequest},
};
use rbit_storage::{
    Database, EpochSessionStore, EpochSnapshot, KvStore, NewUser, SessionRecord,
    sqlite::{credential_epochs::allocate_in, credential_revocations::RevocationIntent, writer_compatibility::register},
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub database: Database,
    pub kv: KvStore,
    pub sessions: EpochSessionStore,
    pub pair: SessionPairIdentity,
    pub raw: SqlitePool,
    pub account: AccountId,
    pub original: CredentialEpoch,
    pub current: EpochSnapshot,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recovery.db");
        let database = Database::open(&path).await.unwrap();
        let kv = KvStore::open(&directory.path().join("sessions.redb")).unwrap();
        let sessions = kv.sessions().epoch_sidecar();
        let pair = sessions.initialize_identity_if_empty().await.unwrap();
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
        let raw = SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(path).foreign_keys(true))).await.unwrap();
        let mut tx = raw.begin().await.unwrap();
        let original = allocate_in(&mut tx).await.unwrap();
        sqlx::query("UPDATE user_logins SET authentication_epoch=?").bind(original.get()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let now = Timestamp::now();
        let record = SessionRecord::at(user.id(), Timestamp(now.0 + 3600), None, "fixture", now);
        let current = sessions.put_bound([1; 32], record.clone(), user.id(), original).await.unwrap().unwrap();
        sessions.put_bound([2; 32], record, user.id(), original).await.unwrap().unwrap();
        Self { pair, directory, database, kv, sessions, raw, account: user.id(), original, current }
    }
    pub async fn keep_change(&self) -> RevocationIntent {
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
    pub async fn request(&self, intent: &RevocationIntent) -> SessionRevocationRequest {
        self.database.credential_revocations().find(intent).await.unwrap().unwrap().request().unwrap()
    }
    pub async fn end_all_change(&self, expected: CredentialEpoch) -> RevocationIntent {
        self.database
            .profiles()
            .set_password_hash_with_intent(self.account, "admin PHC", expected, Timestamp::now())
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
}
