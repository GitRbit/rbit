use super::super::*;
use crate::external_auth::{FLOW_TTL_SECONDS, VerifiedIdentity};
use rbit_core::{AccountId, Config, config::ProviderKind};
use rbit_identity::accounts::ProviderAuthentication;
use rbit_storage::{Database, KvStore, NewUser};
use std::sync::Arc;
const SIGN_IN_PROVIDER: &str = "sso";
pub(super) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
    pub current: OriginalResolvedSession,
    last: Mutex<Option<Flow>>,
}
impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().to_path_buf();
        config.auth.providers = vec![ProviderConfig {
            id: "github".into(),
            name: "GitHub".into(),
            kind: ProviderKind::Github,
            client_id: "fixture".into(),
            client_secret: "fixture".into(),
            ..ProviderConfig::default()
        }];
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let sessions = kv.sessions().epoch_sidecar();
        sessions.initialize_identity_if_empty().await.unwrap();
        database.credential_revocations().initialize_pair(&sessions).await.unwrap();
        let created = database
            .accounts()
            .create_user_original(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "no crypto in this fixture".into(),
                is_admin: false,
            })
            .await
            .unwrap();
        // Sign in through a different provider: the schema allows one identity per provider
        // per account, so the configured GitHub slot must stay free for genuine additions.
        let identity = NewExternalIdentity { provider_id: SIGN_IN_PROVIDER, subject: "source", email: None, username: None };
        database.external_identities().link(created.user.id(), identity, Timestamp(1)).await.unwrap().unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let store = database.external_identities();
        let (_, origin) = ProviderAuthentication::capture_linked(&store, SIGN_IN_PROVIDER, "source").await.unwrap().unwrap();
        let proven = ProviderAuthentication::first_factor(origin).unwrap();
        let current = services.accounts.start_original_session_for(proven.receipt(), &client()).await.unwrap().session;
        Self { _directory: directory, database, services, current, last: Mutex::new(None) }
    }
    pub async fn fresh_session(&self) -> OriginalResolvedSession {
        let (_, origin) = ProviderAuthentication::capture_linked(&self.database.external_identities(), SIGN_IN_PROVIDER, "source")
            .await
            .unwrap()
            .unwrap();
        let proven = ProviderAuthentication::first_factor(origin).unwrap();
        self.services.accounts.start_original_session_for(proven.receipt(), &client()).await.unwrap().session
    }
    pub async fn verified(&self, subject: &str) -> OriginalLinkCallback {
        self.verified_for(subject, &self.current).await
    }
    pub async fn verified_for(&self, subject: &str, current: &OriginalResolvedSession) -> OriginalLinkCallback {
        let (_, sealed) = self.services.external_auth.begin_original_link("github", "/settings/security", current).await.unwrap();
        let flow = self.services.external_auth.key().await.unwrap().open(&sealed, Timestamp::now().seconds(), FLOW_TTL_SECONDS).unwrap();
        *self.last.lock().unwrap() = Some(flow.clone());
        self.verified_from_flow(&flow, subject)
    }
    pub fn last_flow(&self) -> Flow {
        self.last.lock().unwrap().as_ref().unwrap().clone()
    }
    /// Models ONLY the trusted post-remote-verification seam. Other fixtures exercise
    /// cookie/state rejection; this does not claim a real provider integration run.
    pub fn verified_from_flow(&self, flow: &Flow, subject: &str) -> OriginalLinkCallback {
        self.services.external_auth.inner.original_links.verified(&flow.state, callback(subject, self.current.user().user.id())).unwrap()
    }
}
pub(super) fn callback(subject: &str, account: AccountId) -> Callback {
    Callback {
        identity: VerifiedIdentity {
            provider_id: "github".into(),
            subject: subject.into(),
            email: None,
            email_verified: false,
            username: None,
            display_name: None,
            mfa: false,
        },
        next: "/settings/security".into(),
        link_account: Some(account),
    }
}
pub(super) fn client() -> ClientInfo {
    ClientInfo { ip: None, user_agent: "fixture".into() }
}
