//! Identity/session regression group against real `SQLite` and redb stores: forced
//! interleavings for session renewal and released-together contenders for second factors.

use std::sync::Arc;

use rbit_auth::SessionToken;
use rbit_core::{Config, User};
use rbit_identity_models::sessions::ClientInfo;
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};

mod gate;
mod second_factor;
mod sessions;

const PASSWORD: &str = "correct horse battery";
const NEW_PASSWORD: &str = "a whole new passphrase";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    kv: KvStore,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(config, &database, &kv).await.unwrap();
        Self { _dir: dir, database, kv, services }
    }

    async fn user(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, None).await.unwrap()
    }

    async fn session(&self, user: &User) -> SessionToken {
        self.services.accounts.start_session_for(user, &client()).await.unwrap()
    }

    /// A fresh request through the production service path.
    async fn is_live(&self, token: &SessionToken) -> bool {
        self.services.accounts.resume_session(token).await.unwrap().is_some()
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("192.0.2.44".parse().unwrap()), user_agent: "race".to_owned() }
}
