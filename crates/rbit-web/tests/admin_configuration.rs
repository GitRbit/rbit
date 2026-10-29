//! The administrator configures the instance using ordinary HTML forms, without JavaScript.
use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{Config, User};
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    services: Services,
    router: Router,
    admin: User,
    cookie: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let admin = services
            .accounts
            .create_user(
                SignUp { username: "root".into(), email: "root@example.com".into(), password: "correct horse battery".into() },
                Some(true),
            )
            .await
            .unwrap();
        let token = services.accounts.start_session(&admin).await.unwrap();
        let cookie = format!("rbit_session={}", token.encode());
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, database, services, router, admin, cookie }
    }

    async fn get(&self, section: &str) -> Response {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/admin/configuration?section={section}"))
                    .header(header::COOKIE, &self.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn post(&self, inputs: &BTreeMap<String, String>) -> Response {
        let body = inputs
            .iter()
            .map(|(key, value)| format!("{}={}", utf8_percent_encode(key, NON_ALPHANUMERIC), utf8_percent_encode(value, NON_ALPHANUMERIC)))
            .collect::<Vec<_>>()
            .join("&");
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/configuration")
                    .header(header::COOKIE, &self.cookie)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::HOST, "localhost:3000")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn saved(&self) -> serde_json::Value {
        let document =
            self.database.instance_settings().load().await.unwrap().into_iter().find(|(key, _)| key == "deployment_config").unwrap().1;
        let mut value = serde_json::from_str(&document).unwrap();
        // Secret fields are stored sealed; compare them as the server reads them.
        self.database.sealing_key().open_secret_fields(&mut value).unwrap();
        value
    }
}

async fn html(response: Response) -> String {
    String::from_utf8(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

fn mail_inputs() -> BTreeMap<String, String> {
    [
        ("section", "mail"),
        ("action", "save"),
        ("transport", "smtp"),
        ("from_address", "git@example.com"),
        ("from_name", "Company Git"),
        ("smtp_host", "smtp.example.com"),
        ("smtp_port", "587"),
        ("smtp_security", "starttls"),
        ("smtp_username", "git@example.com"),
        ("smtp_password", ""),
        ("timeout_seconds", "30"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect()
}

#[tokio::test]
async fn every_section_renders_labeled_controls_without_a_json_editor() {
    let fixture = Fixture::new().await;
    for section in [
        "server",
        "storage",
        "security",
        "repository",
        "mail",
        "ssh",
        "auth",
        "branding",
        "notifications",
        "operations",
        "api",
        "webhooks",
        "releases",
        "mirrors",
        "push_mirrors",
        "imports",
        "issues",
        "pulls",
        "editor",
        "lfs",
        "actions",
    ] {
        let response = fixture.get(section).await;
        assert_eq!(response.status(), StatusCode::OK, "{section}");
        let markup = html(response).await;
        assert!(markup.contains("Configuration sections") && markup.contains("aria-describedby=\"hint-"));
        assert!(!markup.contains("name=\"document\"") && !markup.contains("Deployment configuration (JSON)"));
        if section == "storage" {
            assert!(markup.contains("readonly") && !markup.contains("Save Storage settings"));
        }
        if section == "mail" {
            assert!(markup.contains("<select") && markup.contains("type=\"password\""));
        }
        if section == "auth" {
            assert!(markup.contains("Add identity provider"));
        }
    }
    assert_eq!(fixture.get("not-a-section").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn section_saves_preserve_unrelated_settings_and_handle_credentials() {
    let fixture = Fixture::new().await;
    let mut initial: serde_json::Value =
        serde_json::from_str(&fixture.services.admin.configuration(&fixture.admin).await.unwrap()).unwrap();
    initial["repository"]["default_branch"] = "trunk".into();
    initial["mail"]["smtp_password"] = "original-smtp-secret".into();
    fixture.services.admin.stage_configuration(&fixture.admin, &initial.to_string()).await.unwrap();
    // Offline storage moves leave old paths in previously saved deployment snapshots.
    initial["storage"]["data_dir"] = "old-data-location".into();
    fixture.database.instance_settings().save(&[("deployment_config", &initial.to_string())]).await.unwrap();
    assert!(!html(fixture.get("storage").await).await.contains("old-data-location"));
    let mut inputs = mail_inputs();
    let response = fixture.post(&inputs).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/admin/configuration?section=mail&saved=1");
    let saved = fixture.saved().await;
    assert_eq!(saved["repository"]["default_branch"], "trunk");
    assert_eq!(saved["mail"]["smtp_password"], "original-smtp-secret");
    assert_eq!(saved["mail"]["smtp_port"], 587);
    assert_ne!(saved["storage"]["data_dir"], "old-data-location");
    let markup = html(fixture.get("mail").await).await;
    assert!(markup.contains("Configured") && !markup.contains("original-smtp-secret") && !markup.contains("&lt;unchanged&gt;"));
    inputs.insert("smtp_password".into(), "replacement-secret".into());
    assert_eq!(fixture.post(&inputs).await.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await["mail"]["smtp_password"], "replacement-secret");
    inputs.insert("smtp_password".into(), String::new());
    inputs.insert("clear.smtp_password".into(), "true".into());
    assert_eq!(fixture.post(&inputs).await.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await["mail"]["smtp_password"], "");
}

#[tokio::test]
async fn validation_preserves_ordinary_input_without_echoing_new_secrets_or_persisting_changes() {
    let fixture = Fixture::new().await;
    let mut inputs = mail_inputs();
    inputs.insert("smtp_port".into(), "70000".into());
    inputs.insert("from_name".into(), "Keep this sender name".into());
    inputs.insert("smtp_password".into(), "never-echo-this-secret".into());
    let response = fixture.post(&inputs).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let markup = html(response).await;
    assert!(markup.contains("Keep this sender name") && markup.contains("SMTP port must be a whole number"));
    assert!(!markup.contains("never-echo-this-secret"));
    assert_eq!(fixture.database.instance_settings().load().await.unwrap(), [] as [(String, String); 0]);
    inputs.insert("smtp_port".into(), "587".into());
    inputs.insert("smtp_host".into(), String::new());
    let response = fixture.post(&inputs).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!html(response).await.contains("never-echo-this-secret"));
}

#[tokio::test]
async fn provider_add_and_remove_are_unsaved_drafts_until_save() {
    let fixture = Fixture::new().await;
    let mut inputs: BTreeMap<String, String> =
        [("section", "auth"), ("timeout_seconds", "10"), ("provider_count", "0"), ("action", "add_provider")]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
    let response = fixture.post(&inputs).await;
    assert_eq!(response.status(), StatusCode::OK);
    let markup = html(response).await;
    assert!(markup.contains("providers.0.client_secret") && markup.contains("Remove provider"));
    assert_eq!(fixture.database.instance_settings().load().await.unwrap(), [] as [(String, String); 0]);
    for (key, value) in [
        ("provider_count", "1"),
        ("action", "save"),
        ("providers.0.kind", "github"),
        ("providers.0.id", "company"),
        ("providers.0.name", "Company SSO"),
        ("providers.0.issuer", ""),
        ("providers.0.base_url", ""),
        ("providers.0.client_id", "client-id"),
        ("providers.0.client_secret", "sso-private-secret"),
        ("providers.0.scopes", ""),
        ("default.providers.0.scopes", "true"),
        ("providers.0.allow_registration", "false"),
    ] {
        inputs.insert(key.into(), value.into());
    }
    assert_eq!(fixture.post(&inputs).await.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await["auth"]["providers"][0]["client_secret"], "sso-private-secret");
    assert!(!html(fixture.get("auth").await).await.contains("sso-private-secret"));
    inputs.insert("providers.0.client_secret".into(), String::new());
    inputs.insert("action".into(), "remove_provider:0".into());
    assert_eq!(fixture.post(&inputs).await.status(), StatusCode::OK);
    assert_eq!(fixture.saved().await["auth"]["providers"].as_array().unwrap().len(), 1);
    inputs.retain(|key, _| !key.starts_with("providers.") && !key.starts_with("default.providers."));
    inputs.insert("provider_count".into(), "0".into());
    inputs.insert("action".into(), "save".into());
    assert_eq!(fixture.post(&inputs).await.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await["auth"]["providers"].as_array().unwrap().as_slice(), [] as [serde_json::Value; 0]);
}

async fn initially_blocked_credentials(fixture: &Fixture, user: &User, initial: &str) -> (String, String) {
    // A credential cannot bypass the browser restriction through Git or the API.
    assert!(matches!(
        fixture
            .services
            .authenticate_git_password(user.name().as_str(), initial.into(), &rbit_services::accounts::ClientInfo::default())
            .await,
        Err(rbit_services::ServiceError::Forbidden)
    ));
    let (_, token) = fixture
        .services
        .tokens
        .create(
            user,
            rbit_services::CreateAccessToken { name: "test credential".into(), scope: rbit_core::TokenScope::Write, expires_in_days: None },
        )
        .await
        .unwrap();
    assert!(fixture.services.tokens.authenticate(&token).await.unwrap().is_none());
    let key = fixture
        .services
        .ssh_keys
        .add_user_key(
            user,
            rbit_services::ssh_keys::AddSshKey {
                title: "test key".into(),
                key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAU5O8WQTG+ptksHlmxF+ieJyp0kwsQ2tQsz0JAuTWme".into(),
            },
        )
        .await
        .unwrap();
    assert!(fixture.services.ssh_keys.find_by_fingerprint(&key.fingerprint).await.unwrap().is_none());

    (token, key.fingerprint)
}

fn session_cookie(response: &Response) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("rbit_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn administrator_created_users_must_replace_their_initial_password_before_using_rbit() {
    let fixture = Fixture::new().await;
    let initial = "initial administrator password";
    let replacement = "my own replacement password";
    let user = fixture
        .services
        .admin
        .create_user(
            &fixture.admin,
            SignUp { username: "newmember".into(), email: "newmember@example.com".into(), password: initial.into() },
            false,
        )
        .await
        .unwrap();
    assert!(fixture.services.accounts.password_change_required(user.id()).await.unwrap());
    assert!(!fixture.services.accounts.password_change_required(fixture.admin.id()).await.unwrap());

    let response = fixture
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("login=newmember&password=initial+administrator+password&next=%2Fnew"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/settings/account");
    let cookie = session_cookie(&response);

    for (method, path) in
        [("GET", "/"), ("GET", "/new"), ("GET", "/settings/tokens"), ("POST", "/settings/account/email"), ("POST", "/new")]
    {
        let response = fixture
            .router
            .clone()
            .oneshot(Request::builder().method(method).uri(path).header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{method} {path}");
        assert_eq!(response.headers()[header::LOCATION], "/settings/account");
    }
    let response = fixture
        .router
        .clone()
        .oneshot(Request::builder().uri("/settings/account").header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content = html(response).await;
    assert!(content.contains("Update your password"));
    assert!(content.contains("before continuing"));
    assert!(!content.contains("action=\"/settings/account/email\""));

    let (token, fingerprint) = initially_blocked_credentials(&fixture, &user, initial).await;

    for (new, confirmation, succeeds) in
        [(initial, initial, false), (replacement, "does not match", false), (replacement, replacement, true)]
    {
        let body = [("current_password", initial), ("new_password", new), ("confirm_password", confirmation)]
            .into_iter()
            .map(|(key, value)| format!("{key}={}", utf8_percent_encode(value, NON_ALPHANUMERIC)))
            .collect::<Vec<_>>()
            .join("&");
        let response = fixture
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/settings/account/password")
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), if succeeds { StatusCode::SEE_OTHER } else { StatusCode::UNPROCESSABLE_ENTITY });
        assert_eq!(fixture.services.accounts.password_change_required(user.id()).await.unwrap(), !succeeds);
    }
    let response = fixture
        .router
        .clone()
        .oneshot(Request::builder().uri("/new").header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the initiating session survives and can use rbit");
    assert!(fixture.services.accounts.authenticate("newmember", initial.into()).await.is_err());
    assert!(fixture.services.accounts.authenticate("newmember", replacement.into()).await.is_ok());
    assert!(fixture.services.tokens.authenticate(&token).await.unwrap().is_some());
    assert!(fixture.services.ssh_keys.find_by_fingerprint(&fingerprint).await.unwrap().is_some());
}
