//! Native language settings and shared-page metadata use the real session projection.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use rbit_core::{Config, User};
use rbit_identity_models::AccountLocale;
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore, Work, count_work};
use std::sync::Arc;
use tower::ServiceExt;

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
    router: Router,
    user: User,
    cookie: String,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.server.site_name = "Forge <script> & friends".into();
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp {
                    username: "language-owner".into(),
                    email: "language@example.com".into(),
                    password: "correct horse battery".into(),
                },
                Some(false),
            )
            .await
            .unwrap();
        let cookie = format!("rbit_session={}", services.accounts.start_session(&user).await.unwrap().encode());
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _dir: dir, services, router, user, cookie }
    }
    async fn get(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
    }
    async fn save(&self, form: &str, htmx: bool) -> Response {
        let mut request = Request::builder()
            .method("POST")
            .uri("/settings/language")
            .header(header::COOKIE, format!("{}; rbit-locale=en", self.cookie))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000");
        if htmx {
            request = request.header("hx-request", "true");
        }
        self.router.clone().oneshot(request.body(Body::from(form.to_owned())).unwrap()).await.unwrap()
    }
}
async fn html(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn browser_spanish_translates_chrome_and_marks_legacy_content_honestly() {
    let fixture = Fixture::new().await;
    let response = fixture.get("/login", &[("accept-language", "es-MX, en;q=0.5")]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "es");
    assert_eq!(response.headers()["rbit-catalog-key"], "1.es");
    let body = html(response).await;
    assert!(body.contains("<html lang=\"es\" dir=\"ltr\""));
    assert!(body.contains("Saltar al contenido") && body.contains("Iniciar sesión"));
    assert!(!body.contains("Forge <script>"), "dynamic translated chrome still uses ordinary template escaping");
    assert!(body.contains("<main id=\"main\" lang=\"en\" dir=\"ltr\""));
    let default = fixture.get("/login", &[]).await;
    assert_eq!(default.headers()[header::CONTENT_LANGUAGE], "en");
    let pseudo = fixture.get("/login", &[("accept-language", "ar-XB"), ("rbit-locale", "ar-XB")]).await;
    assert_eq!(pseudo.headers()[header::CONTENT_LANGUAGE], "en");
}

#[tokio::test]
async fn native_and_htmx_settings_save_canonical_choices_and_reset_to_browser_without_cache_purge() {
    let fixture = Fixture::new().await;
    fixture.services.preferences.set_theme(&fixture.user, "dark").await.unwrap();
    let saved = fixture.save("locale=es", false).await;
    assert_eq!(saved.status(), StatusCode::SEE_OTHER);
    assert_eq!(saved.headers()[header::LOCATION], "/settings/language?saved=1");
    assert_eq!(saved.headers()[header::CACHE_CONTROL], "no-store");
    assert!(saved.headers().get_all(header::SET_COOKIE).iter().any(|value| value.to_str().unwrap().starts_with("rbit-locale=;")));
    assert!(!saved.headers().contains_key("clear-site-data"));
    assert_eq!(fixture.services.preferences.locale(&fixture.user).await.unwrap(), Some(AccountLocale::Spanish));
    let cookies = format!("{}; rbit-locale=en", fixture.cookie);
    let (response, work) = count_work(fixture.get("/settings/language", &[("cookie", &cookies), ("accept-language", "en")])).await;
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 }, "settings selection reuses the session projection");
    assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "es", "account choice wins over browser");
    let body = html(response).await;
    assert!(body.contains("value=\"es\" lang=\"es\" selected") && body.contains("hx-boost=\"false\""));
    assert!(body.contains("Idioma") && body.contains("lang=\"es\" dir=\"ltr\""));
    for invalid in ["locale=ar-XB", "locale=en-XA", "locale=es-MX", "locale=EN", "locale=%3Cscript%3E"] {
        assert_eq!(fixture.save(invalid, false).await.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fixture.services.preferences.locale(&fixture.user).await.unwrap(), Some(AccountLocale::Spanish));
    }
    let reset = fixture.save("locale=", true).await;
    assert_eq!(reset.status(), StatusCode::OK);
    assert_eq!(reset.headers()["hx-redirect"], "/settings/language?saved=1");
    assert_eq!(fixture.services.preferences.locale(&fixture.user).await.unwrap(), None);
    assert_eq!(fixture.services.preferences.theme(&fixture.user).await.unwrap().as_deref(), Some("dark"));
    let response = fixture.get("/settings/language", &[("cookie", &fixture.cookie), ("accept-language", "es")]).await;
    assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "es");
}

#[tokio::test]
async fn catalog_validators_and_refresh_guard_preserve_private_html_and_raw_boundaries() {
    let fixture = Fixture::new().await;
    let english = fixture.get("/login", &[("accept-language", "en")]).await;
    let spanish = fixture.get("/login", &[("accept-language", "es")]).await;
    let tag = spanish.headers()[header::ETAG].to_str().unwrap().to_owned();
    assert_ne!(english.headers()[header::ETAG], spanish.headers()[header::ETAG]);
    let validated = fixture.get("/login", &[("accept-language", "es"), ("if-none-match", &tag)]).await;
    assert_eq!(validated.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(validated.headers()[header::CONTENT_LANGUAGE], "es");
    assert_eq!(validated.headers()["rbit-catalog-key"], "1.es");
    assert!(validated.headers().contains_key(header::CONTENT_SECURITY_POLICY));
    assert_eq!(html(validated).await, "");
    let mismatch = fixture.get("/login", &[("accept-language", "es"), ("hx-request", "true"), ("rbit-document-locale", "1.en")]).await;
    assert_eq!(mismatch.headers()["hx-refresh"], "true");
    assert!(mismatch.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));
    for headers in [
        vec![("hx-request", "true"), ("rbit-document-locale", "1.es")],
        vec![("hx-request", "true"), ("hx-preloaded", "true"), ("rbit-document-locale", "1.en")],
    ] {
        let mut headers = headers;
        headers.push(("accept-language", "es"));
        let response = fixture.get("/login", &headers).await;
        assert!(!response.headers().contains_key("hx-refresh"));
        let expected = if headers.iter().any(|(name, _)| *name == "hx-preloaded") { "private, max-age=30" } else { "private, no-cache" };
        assert_eq!(response.headers()[header::CACHE_CONTROL], expected);
    }
    for path in ["/healthz", "/does-not-exist"] {
        let response = fixture.get(path, &[("accept-language", "es"), ("hx-request", "true"), ("rbit-document-locale", "1.en")]).await;
        assert!(!response.headers().contains_key("hx-refresh"), "raw/error responses must not reload");
    }
}

#[tokio::test]
async fn shared_picker_negotiates_spanish_before_validators_without_new_account_queries() {
    let fixture = Fixture::new().await;
    let (english, english_work) = count_work(fixture.get("/themes/picker", &[("accept-language", "en")])).await;
    let english_tag = english.headers().get(header::ETAG).cloned();
    let english_html = html(english).await;
    let (spanish, spanish_work) = count_work(fixture.get("/themes/picker", &[("accept-language", "es")])).await;
    assert_eq!(english_work, spanish_work);
    assert_eq!(spanish.headers()[header::CONTENT_LANGUAGE], "es");
    assert!(spanish.headers()[header::VARY].to_str().unwrap().contains("Accept-Language"));
    if let Some(tag) = english_tag {
        assert_ne!(spanish.headers().get(header::ETAG), Some(&tag));
    }
    let spanish_html = html(spanish).await;
    assert!(spanish_html.contains("aria-label=\"Elegir un tema\""));
    assert!(english_html.contains("aria-label=\"Choose a theme\""));
    for value in ["dark", "light", "colorblind"] {
        let filter = format!("data-tpk-variant=\"{value}\"");
        assert!(english_html.contains(&filter) && spanish_html.contains(&filter));
    }
}
