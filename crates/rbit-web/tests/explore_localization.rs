//! Real public-service, SQLite, locale, cache and compiled-template integration.

use std::sync::Arc;

use axum::{
    Extension,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use rbit_core::{Config, Timestamp, Visibility};
use rbit_localization::{Locale, LocaleResolver};
use rbit_services::{Services, SignUp};
use rbit_storage::{Database, KvStore, NewRepository};
use tower::ServiceExt;

struct Fixture {
    _dir: tempfile::TempDir,
    router: axum::Router,
    session: String,
}

impl Fixture {
    async fn new(diagnostics: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.server.site_name = "Forge <test> & friends".into();
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp { username: "catalog-owner".into(), email: "catalog@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        for number in 0..32 {
            let repository = database
                .repositories()
                .create(NewRepository {
                    owner_id: user.id(),
                    name: format!("public-{number:02}").parse().unwrap(),
                    description: Some("description <script>\"&data".into()),
                    visibility: Visibility::Public,
                    default_branch: "main".into(),
                })
                .await
                .unwrap();
            if number == 0 {
                database.repo_settings().archive(repository.id, user.id(), Timestamp::now()).await.unwrap();
            }
        }
        database
            .repositories()
            .create(NewRepository {
                owner_id: user.id(),
                name: "private-secret".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".into(),
            })
            .await
            .unwrap();
        let session =
            format!("{}={}", rbit_web::middleware::SESSION_COOKIE, services.accounts.start_session(&user).await.unwrap().encode());
        let mut router = rbit_web::router(rbit_web::AppState::new(config, services));
        if diagnostics {
            router = router.layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
        }
        Self { _dir: dir, router, session }
    }

    async fn get(&self, path: &str, headers: &[(&str, &str)]) -> (HeaderMap, String) {
        let mut request = Request::builder().uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (headers, String::from_utf8(body.to_vec()).unwrap())
    }
}

fn results(html: &str) -> &str {
    let suffix = html.split_once("<div id=\"explore-results\"").unwrap().1;
    suffix.split_once("</main>").map_or(suffix, |(component, _)| component).trim()
}

fn vary(headers: &HeaderMap) -> String {
    headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(",")
}

#[tokio::test]
async fn full_page_and_direct_fragment_share_localized_results_and_private_cache_contracts() {
    let fixture = Fixture::new(true).await;
    let path = "/explore?sort=name&archived=false";
    for (language, direction) in [("en", "ltr"), ("es", "ltr"), ("en-XA", "ltr"), ("ar-XB", "rtl")] {
        let (full_headers, full) = fixture.get(path, &[("Accept-Language", language)]).await;
        let (fragment_headers, fragment) =
            fixture.get(path, &[("Accept-Language", language), ("HX-Request", "true"), ("HX-Target", "explore-results")]).await;
        assert!(full.contains(&format!("<html lang=\"{language}\" dir=\"{direction}\"")));
        assert!(fragment.contains(&format!("lang=\"{language}\" dir=\"{direction}\"")));
        assert!(!fragment.contains("<html") && !fragment.contains("<head>") && !fragment.contains("<form"));
        assert!(fragment.starts_with("<title>"), "HTMX receives the updated title");
        assert_eq!(results(&full), results(&fragment));
        assert!(fragment.len() < full.len(), "fragment omits the layout instead of cutting a rendered layout");
        for headers in [&full_headers, &fragment_headers] {
            assert_eq!(headers[header::CONTENT_LANGUAGE], language);
            assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
            for dimension in ["Accept-Language", "Cookie", "HX-Target", "HX-History-Restore-Request", "HX-Request"] {
                assert!(vary(headers).contains(dimension), "{dimension}");
            }
        }
        assert_ne!(full_headers[header::ETAG], fragment_headers[header::ETAG]);
        assert!(!full.contains("private-secret") && !fragment.contains("private-secret"));
    }
    let (english, _) = fixture.get(path, &[("Accept-Language", "en")]).await;
    let (rtl, _) = fixture.get(path, &[("Accept-Language", "ar-XB")]).await;
    assert_ne!(english[header::ETAG], rtl[header::ETAG], "locale changes validators");
    let (_, signed_in) = fixture.get(path, &[("Cookie", &fixture.session)]).await;
    assert!(!signed_in.contains("private-secret"), "Explore stays public even for the private repository owner");
    let (login_headers, login) = fixture.get("/login", &[("Accept-Language", "ar-XB"), ("Cookie", "rbit-locale=en-XA")]).await;
    assert!(login.contains("<html lang=\"en\" dir=\"ltr\""), "diagnostic preferences do not partially localize unrelated pages");
    assert_eq!(login_headers[header::CONTENT_LANGUAGE], "en");
    assert!(vary(&login_headers).contains("Accept-Language"), "shared chrome negotiates while legacy content is explicitly English");
}

#[tokio::test]
async fn ordinary_get_history_restore_and_poisoned_shape_headers_receive_complete_documents() {
    let fixture = Fixture::new(false).await;
    for headers in [
        vec![],
        vec![("HX-Request", "true")],
        vec![("HX-Request", "true"), ("HX-Target", "explore-results"), ("HX-History-Restore-Request", "true")],
        vec![("HX-Request", "true"), ("HX-Target", "explore-results"), ("HX-Boosted", "true")],
        vec![("HX-Request", "false"), ("HX-Target", "explore-results")],
        vec![("HX-Request", "true"), ("HX-Target", "explore-results"), ("HX-Target", "elsewhere")],
    ] {
        let (_, html) = fixture.get("/explore?sort=name&archived=false&page=2", &headers).await;
        assert!(html.contains("<!doctype html>") && html.contains("<form method=\"get\" action=\"/explore\""));
        assert!(html.contains("name=\"sort\" value=\"name\"") && html.contains("name=\"archived\" value=\"false\""));
        assert!(html.contains("Page 2 of 2") && html.contains("rel=\"prev\""));
    }
    let (headers, html) =
        fixture.get("/explore", &[("Accept-Language", "ar-XB"), ("Cookie", "rbit-locale=en-XA"), ("Rbit-Locale", "ar-XB")]).await;
    assert_eq!(headers[header::CONTENT_LANGUAGE], "en");
    assert!(html.contains("<html lang=\"en\" dir=\"ltr\""), "incoming headers cannot enable diagnostics");
}

#[tokio::test]
async fn malicious_text_filters_and_canonical_query_semantics_survive_both_shapes() {
    let fixture = Fixture::new(true).await;
    for shape in [vec![], vec![("HX-Request", "true"), ("HX-Target", "explore-results")]] {
        let (_, listed) = fixture.get("/explore?sort=name&archived=true", &shape).await;
        assert!(listed.contains("1 repository") && listed.contains("/catalog-owner/public-00"));
        assert!(!listed.contains("/catalog-owner/public-01"));
        assert!(listed.contains(">Archived</span>"));
        assert!(!listed.contains("description <script>"));
        let (_, searched) = fixture.get("/explore?q=%3Cscript%3E%22%26&sort=name&archived=true", &shape).await;
        assert!(!searched.contains("<script>\"&"));
        assert!(searched.contains("sort=name") && searched.contains("archived=true"));
        assert!(searched.contains("q=%3Cscript%3E%22%26"), "filter links preserve and encode the search");
        let (_, empty) = fixture.get("/explore?q=no-match-private-secret", &shape).await;
        assert!(empty.contains("No matching repositories") && !empty.contains("href=\"/catalog-owner/private-secret\""));
        let (_, fallback) = fixture.get("/explore?sort=unknown&archived=maybe&page=invalid", &shape).await;
        assert!(fallback.contains("32 repositories"), "lenient query defaults are preserved");
    }
    let (_, no_js) = fixture.get("/explore?q=%3Cscript%3E%22%26", &[]).await;
    assert!(!no_js.contains("<test>"), "instance-name interpolation is escaped too");
    assert!(no_js.contains("type=\"submit\"") && no_js.contains("hx-push-url=\"true\""));
}
