use rbit_core::{Config, User};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_services::{
    Services, SignUp,
    accounts::{ClientInfo, OriginalIssuedSession, OriginalProofReceipt},
};
use rbit_storage::{
    Database, KvStore,
    sqlite::{credential_epochs::allocate_in, writer_compatibility::register},
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::sync::Arc;
pub(super) const PASSWORD: &str = "correct horse battery";
pub(super) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub database: Database,
    pub kv: KvStore,
    pub raw: SqlitePool,
    pub services: Services,
    pub alice: User,
    pub original: CredentialEpoch,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().to_path_buf();
        config.security.session_max_age_days = 1;
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = services
            .accounts
            .create_user(SignUp { username: "alice".into(), email: "alice@example.com".into(), password: PASSWORD.into() }, None)
            .await
            .unwrap();
        let raw =
            SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(directory.path().join("rbit.db")).foreign_keys(true)))
                .await
                .unwrap();
        // Isolated fixture models the required stopped upgrade's positive global
        // rebase. This is not production startup/mode activation.
        let mut tx = raw.begin().await.unwrap();
        let original = allocate_in(&mut tx).await.unwrap();
        sqlx::query("UPDATE user_logins SET authentication_epoch=? WHERE account_id=?")
            .bind(original.get())
            .bind(alice.id().0)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        Self { _directory: directory, database, kv, raw, services, alice, original }
    }
    pub async fn proof(&self) -> OriginalProofReceipt {
        let (_, proof) = self.services.accounts.authenticate_original_proof("alice", PASSWORD.into(), |_| Ok(())).await;
        proof.unwrap().into_parts().1
    }
    pub async fn issue(&self) -> OriginalIssuedSession {
        let proof = self.proof().await;
        self.services.accounts.start_original_session_for(&proof, &ClientInfo::default()).await.unwrap()
    }
}
