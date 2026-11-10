//! Instance access settings through the real router, database and templates: turning the
//! Explore page off.
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, User, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery";

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
    router: axum::Router,
    admin: User,
    member: User,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let db = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &db, &kv).await.unwrap();
        let signup = |name: &str| SignUp { username: name.into(), email: format!("{name}@example.com"), password: PASSWORD.into() };
        let admin = services.accounts.create_user(signup("root"), Some(true)).await.unwrap();
        let member = services.accounts.create_user(signup("keeper"), Some(false)).await.unwrap();
        let input = CreateRepository { name: "open".into(), description: "Out in the open".into(), visibility: Visibility::Public };
        services.repositories.create(&member, input).await.unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _dir: dir, services, router, admin, member }
    }

    async fn update(&self, change: impl FnOnce(&mut rbit_core::InstanceSettings)) {
        let mut settings = self.services.instance.current();
        change(&mut settings);
        self.services.admin.update_settings(&self.admin, settings).await.unwrap();
    }

    async fn cookie(&self, user: &User) -> String {
        format!("{}={}", rbit_web::middleware::SESSION_COOKIE, self.services.accounts.start_session(user).await.unwrap().encode())
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
    }

    /// `GET path`, signed in with `cookie` when given.
    async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut request = Request::builder().uri(path);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }
}

fn links_explore(html: &str) -> bool {
    html.contains("href=\"/explore\"") || html.contains("action=\"/explore\"")
}

#[tokio::test]
async fn turning_explore_off_hides_the_page_and_its_links() {
    let fixture = Fixture::new().await;
    let admin = fixture.cookie(&fixture.admin).await;
    let member = fixture.cookie(&fixture.member).await;

    for cookie in [None, Some(member.as_str()), Some(admin.as_str())] {
        let (status, _, html) = fixture.get("/explore", cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("href=\"/keeper/open\""));
        let (_, _, home) = fixture.get("/", cookie).await;
        assert!(links_explore(&home), "the front page links Explore by default");
    }

    fixture.update(|settings| settings.explore_enabled = false).await;
    for cookie in [None, Some(member.as_str()), Some(admin.as_str())] {
        let (status, _, html) = fixture.get("/explore", cookie).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "gone for everyone, administrators included");
        assert!(!links_explore(&html), "not even from the not-found page");
        let (status, _, home) = fixture.get("/", cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!links_explore(&home), "no header, menu, footer or front-page link");
        let (_, _, repo) = fixture.get("/keeper/open", cookie).await;
        assert!(!links_explore(&repo));
    }
    let (status, _, _) = fixture.get("/keeper/open", None).await;
    assert_eq!(status, StatusCode::OK, "public repositories stay public");
    let (_, _, manifest) = fixture.get("/manifest.webmanifest", None).await;
    assert!(!manifest.contains("/explore"), "no app shortcut either");

    let (_, _, settings) = fixture.get("/admin/settings", Some(&admin)).await;
    assert!(settings.contains(r#"name="explore" value="disabled" class="mt-1 accent-(--ink)" checked"#), "the form shows the choice");

    fixture.update(|settings| settings.explore_enabled = true).await;
    let (status, _, _) = fixture.get("/explore", None).await;
    assert_eq!(status, StatusCode::OK, "turning it back on restores it");
}

#[tokio::test]
async fn the_settings_form_turns_explore_off() {
    let fixture = Fixture::new().await;
    let admin = fixture.cookie(&fixture.admin).await;
    let form = "site_name=rbit&registration=open&explore=disabled&org_creation=any_user&default_visibility=public&digest_hour=8";
    let request = Request::builder()
        .method("POST")
        .uri("/admin/settings")
        .header(header::COOKIE, &admin)
        .header(header::ORIGIN, "http://localhost:3000")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let (status, headers, _) = fixture.send(request).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/admin/settings?saved=1");
    assert!(!fixture.services.instance.current().explore_enabled);
    let (status, _, _) = fixture.get("/explore", Some(&admin)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
