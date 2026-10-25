//! Settings → Notifications: what is emailed and when (immediately, or in daily or weekly
//! digests at a chosen hour), through the ordinary HTML form.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{
    Config, User,
    config::{EmailDelivery, EmailPreference},
};
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    router: Router,
    user: User,
    cookie: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.notifications.digest_hour = 7;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp { username: "dana".into(), email: "dana@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        let token = services.accounts.start_session(&user).await.unwrap();
        let cookie = format!("rbit_session={}", token.encode());
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, services, router, user, cookie }
    }

    async fn get(&self) -> String {
        let request = Request::builder().uri("/settings/notifications").header(header::COOKIE, &self.cookie).body(Body::empty()).unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        html(response).await
    }

    async fn post(&self, pairs: &[(&str, &str)]) -> Response {
        let body = pairs
            .iter()
            .map(|(key, value)| format!("{}={}", utf8_percent_encode(key, NON_ALPHANUMERIC), utf8_percent_encode(value, NON_ALPHANUMERIC)))
            .collect::<Vec<_>>()
            .join("&");
        let request = Request::builder()
            .method("POST")
            .uri("/settings/notifications")
            .header(header::COOKIE, &self.cookie)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .body(Body::from(body))
            .unwrap();
        self.router.clone().oneshot(request).await.unwrap()
    }

    async fn saved(&self) -> (EmailPreference, EmailDelivery, Option<u8>) {
        let settings = self.services.notification_email.settings(&self.user).await.unwrap();
        (settings.preference, settings.delivery, settings.digest_hour)
    }
}

async fn html(response: Response) -> String {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

#[tokio::test]
async fn the_form_shows_immediate_delivery_and_the_instance_hour_by_default() {
    let fixture = Fixture::new().await;
    let page = fixture.get().await;
    assert!(page.contains(r#"value="immediate" class="mt-1 accent-(--ink)" checked"#), "{page}");
    for value in ["daily", "weekly"] {
        assert!(page.contains(&format!(r#"name="delivery" value="{value}""#)), "{value}");
    }
    assert!(page.contains(r#"<option value="" selected>Default (07:00 UTC)</option>"#), "the configured default hour");
    assert!(page.contains(r#"<select class="input cursor-pointer appearance-none pr-9" id="digest-hour" name="digest_hour""#));
    assert!(!page.contains("style="), "no inline styles");
}

#[tokio::test]
async fn saving_a_digest_stores_delivery_and_hour() {
    let fixture = Fixture::new().await;
    let response = fixture.post(&[("email", "all"), ("delivery", "weekly"), ("digest_hour", "18")]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/settings/notifications?saved=1");
    assert_eq!(fixture.saved().await, (EmailPreference::All, EmailDelivery::Weekly, Some(18)));
    let page = fixture.get().await;
    assert!(page.contains(r#"value="weekly" class="mt-1 accent-(--ink)" checked"#));
    assert!(page.contains(r#"<option value="18" selected>18:00 UTC</option>"#));

    // Back to the default hour and to immediate emails.
    let response = fixture.post(&[("email", "participating"), ("delivery", "immediate"), ("digest_hour", "")]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await, (EmailPreference::Participating, EmailDelivery::Immediate, None));
}

#[tokio::test]
async fn a_form_without_delivery_keeps_the_current_choice() {
    let fixture = Fixture::new().await;
    fixture.post(&[("email", "all"), ("delivery", "daily"), ("digest_hour", "6")]).await;
    let response = fixture.post(&[("email", "participating")]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.saved().await, (EmailPreference::Participating, EmailDelivery::Daily, None), "the hour field was left empty");
}

#[tokio::test]
async fn invalid_choices_are_explained_and_change_nothing() {
    let fixture = Fixture::new().await;
    for (pairs, message) in [
        (&[("email", "all"), ("delivery", "hourly")][..], "Choose when to send them."),
        (&[("email", "all"), ("delivery", "daily"), ("digest_hour", "24")][..], "Choose a digest time from the list."),
        (&[("email", "all"), ("delivery", "daily"), ("digest_hour", "noon")][..], "Choose a digest time from the list."),
        (&[("email", "sometimes"), ("delivery", "daily")][..], "Choose which notifications to email."),
    ] {
        let response = fixture.post(pairs).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{pairs:?}");
        assert!(html(response).await.contains(message), "{pairs:?}");
    }
    assert_eq!(fixture.saved().await, (EmailPreference::Participating, EmailDelivery::Immediate, None));
}
