use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{
    Database, NewUser,
    sqlite::{credential_revocations::RevocationIntent, writer_compatibility::register},
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

pub(super) const OLD: &str = "original PHC fixture";
pub(super) const NEW: &str = "replacement PHC fixture";
pub(super) const NOW: Timestamp = Timestamp(100);
pub(super) struct Fixture {
    pub directory: tempfile::TempDir,
    pub database: Database,
    pub raw: SqlitePool,
    pub account: AccountId,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct State {
    pub login: (String, i64, i64, Option<i64>),
    pub links: Vec<(Vec<u8>, String, Option<i64>)>,
    pub intents: Vec<(i64, i64, Option<String>)>,
    pub high_water: i64,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credential.db");
        let database = Database::open(&path).await.unwrap();
        let user = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: OLD.into(),
                is_admin: false,
            })
            .await
            .unwrap();
        let raw = SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(path).foreign_keys(true))).await.unwrap();
        // Deliberately multiple honoured links to test one-winner revocation; the
        // ordinary insertion API already replaces older links of the same purpose.
        for (digest, purpose) in [(1_u8, "reset"), (2, "reset"), (3, "verify")] {
            sqlx::query(
                "INSERT INTO email_tokens(digest,account_id,purpose,email,created_at,expires_at) VALUES(?,?,?,'alice@example.com',10,1000)",
            )
            .bind([digest; 32].as_slice())
            .bind(user.id().0)
            .bind(purpose)
            .execute(&raw)
            .await
            .unwrap();
        }
        sqlx::query("UPDATE user_logins SET password_change_required=1").execute(&raw).await.unwrap();
        Self { directory, database, raw, account: user.id() }
    }
    pub async fn state(&self) -> State {
        State {
            login: sqlx::query_as(
                "SELECT password_hash,authentication_epoch,password_change_required,email_verified_at FROM user_logins WHERE account_id=?",
            )
            .bind(self.account.0)
            .fetch_one(&self.raw)
            .await
            .unwrap(),
            links: sqlx::query_as("SELECT digest,purpose,used_at FROM email_tokens ORDER BY digest").fetch_all(&self.raw).await.unwrap(),
            intents: sqlx::query_as("SELECT account_id,target_epoch,outcome FROM credential_revocations ORDER BY id")
                .fetch_all(&self.raw)
                .await
                .unwrap(),
            high_water: sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator").fetch_one(&self.raw).await.unwrap(),
        }
    }
    pub async fn self_change(
        &self,
        expected: &str,
        epoch: CredentialEpoch,
    ) -> Result<Option<RevocationIntent>, rbit_storage::StorageError> {
        self.database.profiles().set_password_hash_if_current_with_intent(self.account, NEW, expected, epoch, NOW).await
    }
    pub async fn reset(&self) -> Result<Option<rbit_storage::sqlite::recovery::PasswordResetMutation>, rbit_storage::StorageError> {
        self.database.recovery().reset_password_with_intent(&[1; 32], NEW, NOW).await
    }
    pub async fn execute(&self, sql: &'static str) {
        sqlx::raw_sql(sql).execute(&self.raw).await.unwrap();
    }
}
