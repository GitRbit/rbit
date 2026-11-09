//! One language form for accounts and visitors, saved to the account preference or, for a
//! visitor, to the `rbit-locale` cookie that negotiation honours before Accept-Language.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use rbit_core::{Config, User};
use rbit_identity_models::AccountLocale;
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore};
use std::sync::Arc;
use tower::ServiceExt;

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
    router: Router,
    user: User,
    session: String,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let signup = SignUp { username: "polyglot".into(), email: "polyglot@example.com".into(), password: "correct horse battery".into() };
        let user = services.accounts.create_user(signup, Some(false)).await.unwrap();
        let session = format!("rbit_session={}", services.accounts.start_session(&user).await.unwrap().encode());
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _dir: dir, services, router, user, session }
    }

    async fn get(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
    }

    /// Posts the language form.
    async fn save(&self, form: &str, cookie: Option<&str>, origin: &str) -> Response {
        let mut request = Request::builder()
            .method("POST")
            .uri("/settings/language")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, origin);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        self.router.clone().oneshot(request.body(Body::from(form.to_owned())).unwrap()).await.unwrap()
    }
}

async fn html(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn set_cookies(response: &Response) -> Vec<String> {
    response.headers().get_all(header::SET_COOKIE).iter().map(|value| value.to_str().unwrap().to_owned()).collect()
}

const SITE: &str = "http://localhost:3000";

#[tokio::test]
async fn a_visitor_choice_is_a_cookie_that_wins_over_accept_language() {
    let fixture = Fixture::new().await;
    let saved = fixture.save("locale=ja", None, SITE).await;
    assert_eq!(saved.status(), StatusCode::SEE_OTHER);
    assert_eq!(saved.headers()[header::LOCATION], "/settings/language?saved=1");
    assert_eq!(saved.headers()[header::CACHE_CONTROL], "no-store");
    let cookies = set_cookies(&saved);
    let cookie = cookies.iter().find(|cookie| cookie.starts_with("rbit-locale=ja;")).expect("the choice is a cookie");
    for part in ["Path=/", "HttpOnly", "SameSite=Lax", "Max-Age=31536000"] {
        assert!(cookie.contains(part), "{cookie} has {part}");
    }
    let page = fixture.get("/explore", &[("accept-language", "de"), ("cookie", "rbit-locale=ja")]).await;
    assert_eq!(page.headers()[header::CONTENT_LANGUAGE], "ja", "the cookie wins over the browser");
    assert!(html(page).await.contains("<html lang=\"ja\""));

    let reset = fixture.save("locale=", Some("rbit-locale=ja"), SITE).await;
    assert_eq!(reset.status(), StatusCode::SEE_OTHER);
    assert!(set_cookies(&reset).iter().any(|cookie| cookie.starts_with("rbit-locale=;")), "following the browser removes it");

    for invalid in ["locale=ar-XB", "locale=zh", "locale=EN"] {
        let refused = fixture.save(invalid, None, SITE).await;
        assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY, "{invalid}");
        assert!(set_cookies(&refused).iter().all(|cookie| !cookie.starts_with("rbit-locale=")));
        assert!(html(refused).await.contains("role=\"alert\""));
    }
    let cross_site = fixture.save("locale=ja", None, "https://evil.example").await;
    assert_eq!(cross_site.status(), StatusCode::FORBIDDEN, "the same origin check as every form");
    assert!(set_cookies(&cross_site).is_empty());
}

#[tokio::test]
async fn a_signed_in_choice_saves_the_account_preference_and_drops_the_cookie() {
    let fixture = Fixture::new().await;
    let cookie = format!("{}; rbit-locale=ja", fixture.session);
    let saved = fixture.save("locale=de", Some(&cookie), SITE).await;
    assert_eq!(saved.status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.services.preferences.locale(&fixture.user).await.unwrap(), Some(AccountLocale::German));
    assert!(set_cookies(&saved).iter().any(|cookie| cookie.starts_with("rbit-locale=;")), "the visitor cookie gives way to the account");
    let page = fixture.get("/explore", &[("cookie", &fixture.session), ("accept-language", "ko")]).await;
    assert_eq!(page.headers()[header::CONTENT_LANGUAGE], "de");
}

#[tokio::test]
async fn visitors_and_accounts_get_a_working_language_form_without_scripts() {
    let fixture = Fixture::new().await;
    let visitor = fixture.get("/settings/language", &[("accept-language", "fr"), ("cookie", "rbit-locale=fr")]).await;
    assert_eq!(visitor.status(), StatusCode::OK, "visitors are not sent to sign in");
    assert_eq!(visitor.headers()[header::CONTENT_LANGUAGE], "fr");
    let body = html(visitor).await;
    assert!(body.contains("<form class=\"box mt-5 p-4\" method=\"post\" action=\"/settings/language\" hx-boost=\"false\">"));
    assert!(body.contains("<option value=\"fr\" lang=\"fr\" selected>Français</option>"));
    assert!(!body.contains("href=\"/settings/profile\""), "no account settings for a visitor");

    let account = html(fixture.get("/settings/language", &[("cookie", &fixture.session)]).await).await;
    assert!(account.contains("href=\"/settings/profile\""), "accounts get their settings page");
    assert!(account.contains("action=\"/settings/language\""));
}
