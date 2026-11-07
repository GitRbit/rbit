use super::{Authenticator, ClientInfo, SecondFactorProof, Services};
use crate::{
    SignUp,
    two_factor::{OriginalSignInOutcome, SignInOutcome},
};
use rbit_auth::{SessionToken, TotpSecret, totp::time_step};
use rbit_core::{Config, Timestamp, User};
use rbit_storage::{Database, KvStore, sqlite::credential_revocations as intents};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{num::NonZeroU8, sync::Arc};
const PASSWORD: &str = "correct horse battery";

#[derive(Clone, Copy, Debug)]
pub(super) enum Factor {
    Recovery,
    Totp,
    Passkey,
}
impl Factor {
    pub const ALL: [Self; 3] = [Self::Recovery, Self::Totp, Self::Passkey];
    pub fn proof(self, answer: &str) -> SecondFactorProof<'_> {
        match self {
            Self::Passkey => SecondFactorProof::Passkey(answer),
            _ => SecondFactorProof::Code(answer),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Change {
    Reset,
    Suspend,
    Pending,
}
pub(super) enum Material {
    Recovery(String),
    Totp(TotpSecret, u64),
    Passkey(Authenticator),
}
impl Material {
    pub async fn answer(&self, fixture: &Fixture, pending: &SessionToken) -> String {
        match self {
            Self::Recovery(code) => code.clone(),
            Self::Totp(secret, step) => format!("{:06}", secret.code_at_step(step + 1)),
            Self::Passkey(key) => key.get(&fixture.services.two_factor.sign_in_options(pending).await.unwrap().unwrap()),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProofState {
    step: Option<u64>,
    passkeys: Vec<(i64, u32, Option<Timestamp>)>,
    recovery: i64,
}
pub(super) struct Fixture {
    _dir: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
    pub raw: SqlitePool,
    pub alice: User,
}
impl Fixture {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new())
                    .filename(config.storage.sqlite_path())
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let mut alice = None;
        for name in ["root", "alice"] {
            let created = services
                .accounts
                .create_user(SignUp { username: name.into(), email: format!("{name}@example.com"), password: PASSWORD.into() }, None)
                .await
                .unwrap();
            if name == "alice" {
                alice = Some(created);
            }
        }
        let alice = alice.unwrap();
        // Synthetic nonzero snapshot isolates a same-epoch pending transition. No
        // allocator/history/activation authority is inferred from this fixture seed.
        sqlx::query("UPDATE user_logins SET authentication_epoch=1 WHERE account_id=?").bind(alice.id().0).execute(&raw).await.unwrap();
        Self { _dir: dir, database, services, raw, alice }
    }
    pub async fn install(&self, factor: Factor) -> Material {
        if matches!(factor, Factor::Passkey) {
            let key = Authenticator::new(31);
            let options = self.services.two_factor.creation_options(&self.alice).await.unwrap();
            self.services.add_passkey(&self.alice, "fixture key", &key.create(&options), &client()).await.unwrap();
            return Material::Passkey(key);
        }
        self.services.two_factor.begin_enrolment(&self.alice, "rbit").await.unwrap();
        let record = self.database.two_factor().find(self.alice.id()).await.unwrap().unwrap();
        let secret = TotpSecret::from_bytes(&record.secret).unwrap();
        let step = time_step(Timestamp::now().seconds());
        let code = format!("{:06}", secret.code_at_step(step));
        let codes = self.services.confirm_two_factor(&self.alice, &code, &client()).await.unwrap().unwrap();
        match factor {
            Factor::Recovery => Material::Recovery(codes[0].clone()),
            _ => Material::Totp(secret, step),
        }
    }
    pub async fn pending(&self, original: bool) -> SessionToken {
        if original {
            match self.services.sign_in_original("alice", PASSWORD.into(), &client()).await.unwrap() {
                OriginalSignInOutcome::SecondFactorRequired(pending) => pending,
                OriginalSignInOutcome::SignedIn(_) => panic!("fixture must require an original second factor"),
            }
        } else {
            match self.services.sign_in("alice", PASSWORD.into(), &client()).await.unwrap() {
                SignInOutcome::SecondFactorRequired(pending) => pending,
                SignInOutcome::SignedIn(_) => panic!("fixture must require a second factor"),
            }
        }
    }
    pub async fn proof_state(&self) -> ProofState {
        let store = self.database.two_factor();
        ProofState {
            step: store.find(self.alice.id()).await.unwrap().and_then(|row| row.last_used_step),
            passkeys: store
                .passkeys(self.alice.id())
                .await
                .unwrap()
                .into_iter()
                .map(|key| (key.id, key.sign_count, key.last_used_at))
                .collect(),
            recovery: store.remaining_recovery_codes(self.alice.id()).await.unwrap(),
        }
    }
    pub async fn change(&self, change: Change) {
        match change {
            Change::Reset => {
                let hash =
                    zeroize::Zeroizing::new(self.services.accounts.password_work().begin().unwrap().hash(PASSWORD.into()).await.unwrap());
                let mut tx = self.raw.begin().await.unwrap();
                sqlx::query("UPDATE user_logins SET password_hash=? WHERE account_id=?")
                    .bind(hash.as_str())
                    .bind(self.alice.id().0)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                intents::stage(&mut tx, self.alice.id(), Timestamp::now()).await.unwrap();
                tx.commit().await.unwrap();
            }
            Change::Suspend => {
                self.database.admin().set_suspended(self.alice.id(), true).await.unwrap();
            }
            Change::Pending => {
                // Fault-isolated same-epoch barrier transition, independent of the
                // reset's epoch change; this is not an activated staging writer.
                sqlx::query("INSERT INTO credential_revocations(account_id,target_epoch,created_at) VALUES(?,1,10)")
                    .bind(self.alice.id().0)
                    .execute(&self.raw)
                    .await
                    .unwrap();
            }
        }
    }
    pub async fn recover(&self, change: Change) {
        if matches!(change, Change::Suspend) {
            self.database.admin().set_suspended(self.alice.id(), false).await.unwrap();
        } else {
            let mut tx = self.raw.begin().await.unwrap();
            let pending = intents::pending(&mut tx, 0, NonZeroU8::new(10).unwrap()).await.unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(
                intents::complete(&mut tx, &pending[0], intents::RevocationOutcome::EndedAll, Timestamp::now()).await.unwrap(),
                intents::Completion::Applied
            );
            tx.commit().await.unwrap();
        }
    }
}
pub(super) fn client() -> ClientInfo {
    ClientInfo { ip: None, user_agent: "fixture".into() }
}
