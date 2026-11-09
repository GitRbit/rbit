//! Real service composition with an explicitly shared password-work owner.

use std::{num::NonZeroUsize, sync::Arc};

use axum::{
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_auth::{SessionToken, TotpSecret, totp::time_step};
use rbit_core::{Config, Timestamp, TokenScope, User, Visibility};
use rbit_identity::PasswordWork;
use rbit_identity_models::sessions::ClientInfo;
use rbit_kernel::BlockingBudget;
use rbit_services::{CreateAccessToken, CreateRepository, Services, SignUp};
use rbit_storage::{
    Database, KvStore,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};
use tower::ServiceExt;

pub(super) const PASSWORD: &str = "protocol-password-123";
pub(super) const LOGIN_FORM: &str = "login=keeper&password=protocol-password-123";

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    pub(super) services: Services,
    pub(super) passwords: PasswordWork,
    router: axum::Router,
    database: Database,
    kv: KvStore,
    config: Arc<Config>,
    user: User,
    pub(super) token: String,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.security.max_password_jobs = 1;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let passwords = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()));
        let services = Services::with_password_work(config.clone(), &database, &kv, passwords.clone()).await.unwrap();
        let user = services
            .accounts
            .create_user(SignUp { username: "keeper".into(), email: "keeper@example.com".into(), password: PASSWORD.into() }, Some(true))
            .await
            .unwrap();
        services
            .repositories
            .create(
                &user,
                CreateRepository {
                    name: "project".into(),
                    description: "Public password-admission protocol fixture".into(),
                    visibility: Visibility::Public,
                },
            )
            .await
            .unwrap();
        let (_, token) = services
            .tokens
            .create(&user, CreateAccessToken { name: "non-password probe".into(), scope: TokenScope::Write, expires_in_days: None })
            .await
            .unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config.clone(), services.clone()));
        Self { _directory: directory, services, passwords, router, database, kv, config, user, token }
    }

    pub(super) fn basic_header() -> String {
        format!("Basic {}", STANDARD.encode(format!("keeper:{PASSWORD}")))
    }

    pub(super) fn basic_pat_header(&self) -> String {
        // Git ignores the Basic username when the password is a personal access token.
        format!("Basic {}", STANDARD.encode(format!("ignored:{}", self.token)))
    }

    pub(super) async fn reset_token(&self) -> String {
        let secret = SessionToken::generate();
        self.database
            .recovery()
            .insert(&NewEmailToken {
                digest: secret.digest(),
                account_id: self.user.id(),
                purpose: TokenPurpose::Reset,
                email: self.user.email.clone(),
                expires_at: Timestamp(Timestamp::now().seconds() + 3600),
            })
            .await
            .unwrap();
        secret.encode()
    }

    /// A closed owner cannot reopen. Recompose a new owner over the same stores,
    /// preserving actual persisted sessions and one-use proofs for retry assertions.
    pub(super) async fn restart_password_owner(&mut self) {
        let passwords = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()));
        let services = Services::with_password_work(self.config.clone(), &self.database, &self.kv, passwords.clone()).await.unwrap();
        self.router = rbit_web::router(rbit_web::AppState::new(self.config.clone(), services.clone()));
        self.services = services;
        self.passwords = passwords;
    }

    pub(super) async fn session(&self) -> SessionToken {
        self.services.accounts.start_session(&self.user).await.unwrap()
    }

    pub(super) async fn session_is_valid(&self, session: &SessionToken) -> bool {
        self.services.accounts.resume_session(session).await.unwrap().is_some()
    }

    pub(super) async fn password_hash(&self) -> String {
        self.database.profiles().password_hash(self.user.id()).await.unwrap().unwrap()
    }

    pub(super) async fn enable_two_factor(&self) -> Vec<String> {
        self.services.two_factor.begin_enrolment(&self.user, "rbit").await.unwrap();
        let record = self.database.two_factor().find(self.user.id()).await.unwrap().unwrap();
        let secret = TotpSecret::from_bytes(&record.secret).unwrap();
        let code = format!("{:06}", secret.code_at_step(time_step(Timestamp::now().seconds())));
        self.services.confirm_two_factor(&self.user, &code, &ClientInfo::default()).await.unwrap().expect("first factor recovery codes")
    }

    pub(super) async fn recovery_codes_left(&self) -> i64 {
        self.services.two_factor.status(&self.user).await.unwrap().recovery_codes_left
    }

    pub(super) async fn two_factor_is_enabled(&self) -> bool {
        self.services.two_factor.is_enabled(self.user.id()).await.unwrap()
    }

    pub(super) async fn signed_form(&self, session: &SessionToken, path: &str, body: &str) -> (StatusCode, HeaderMap, String) {
        let cookie = format!("{}={}", rbit_web::middleware::SESSION_COOKIE, session.encode());
        self.request(
            Method::POST,
            path,
            &[
                (header::CONTENT_TYPE.as_str(), "application/x-www-form-urlencoded"),
                (header::COOKIE.as_str(), &cookie),
                ("HX-Request", "true"),
            ],
            body,
        )
        .await
    }

    pub(super) async fn htmx_form(&self, path: &str, body: &str) -> (StatusCode, HeaderMap, String) {
        self.request(
            Method::POST,
            path,
            &[(header::CONTENT_TYPE.as_str(), "application/x-www-form-urlencoded"), ("HX-Request", "true")],
            body,
        )
        .await
    }

    pub(super) async fn request(
        &self,
        method: Method,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (StatusCode, HeaderMap, String) {
        let mut request = Request::builder().method(method).uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = self.router.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    pub(super) async fn form(&self, path: &str, body: &str) -> (StatusCode, HeaderMap, String) {
        self.request(Method::POST, path, &[(header::CONTENT_TYPE.as_str(), "application/x-www-form-urlencoded")], body).await
    }
}
