//! Plugin controls remain on one router across runtime transitions.

use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt as _;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    #[cfg(any(feature = "actions", feature = "i18n"))]
    database: Database,
    #[cfg(feature = "i18n")]
    config: Arc<Config>,
    #[cfg(feature = "i18n")]
    kv: KvStore,
    router: axum::Router,
    #[cfg(any(feature = "actions", feature = "i18n"))]
    admin: rbit_core::User,
    admin_cookie: String,
    user_cookie: String,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_api(true).await
    }

    async fn with_api(api_enabled: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.api.enabled = api_enabled;
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.updates.check_for_updates = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let admin = services
            .accounts
            .create_user(
                SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() },
                Some(true),
            )
            .await
            .unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        services
            .repositories
            .create(&admin, CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Public })
            .await
            .unwrap();
        let cookie = |token: rbit_auth::SessionToken| format!("rbit_session={}", token.encode());
        let admin_cookie = cookie(services.accounts.start_session(&admin).await.unwrap());
        let user_cookie = cookie(services.accounts.start_session(&user).await.unwrap());
        let router = rbit_web::router(rbit_web::AppState::new(config.clone(), services.clone()));
        Self {
            _directory: directory,
            services,
            #[cfg(any(feature = "actions", feature = "i18n"))]
            database,
            #[cfg(feature = "i18n")]
            config,
            #[cfg(feature = "i18n")]
            kv,
            router,
            #[cfg(any(feature = "actions", feature = "i18n"))]
            admin,
            admin_cookie,
            user_cookie,
        }
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        cookie: Option<&str>,
        origin: &str,
        body: &str,
    ) -> (StatusCode, String, axum::http::HeaderMap) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, origin)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = self.router.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        (parts.status, String::from_utf8(bytes.to_vec()).unwrap(), parts.headers)
    }

    async fn get(&self, path: &str) -> (StatusCode, String, axum::http::HeaderMap) {
        self.request(Method::GET, path, Some(&self.admin_cookie), "http://localhost:3000", "").await
    }
}

#[tokio::test]
async fn plugin_administration_is_host_owned_and_requires_an_administrator() {
    let fixture = Fixture::new().await;
    let (status, body, _) = fixture.get("/admin/plugins").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Plugins"));
    assert_eq!(
        fixture.request(Method::GET, "/admin/plugins", Some(&fixture.user_cookie), "http://localhost:3000", "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(Method::POST, "/admin/plugins/actions", Some(&fixture.user_cookie), "http://localhost:3000", "enabled=true")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(Method::POST, "/admin/plugins/actions", Some(&fixture.admin_cookie), "https://other.example", "enabled=true")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(fixture.services.actions.is_none());
    if !cfg!(feature = "actions") {
        assert!(body.contains("This build does not include Actions"));
    }
}

#[tokio::test]
async fn runtime_refusals_report_the_revision_needed_to_refresh_an_open_page() {
    let fixture = Fixture::new().await;
    let revision = fixture.services.plugin_revision().to_string();
    let (status, _, headers) = fixture.get("/alice/rocket/actions").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers["rbit-plugin-revision"], revision);
    for authorization in [format!("Bearer rbat_{}", "a".repeat(43)), format!("Bearer {}", "a".repeat(4097))] {
        let request =
            Request::builder().uri("/api/v1/repos/alice/rocket").header(header::AUTHORIZATION, authorization).body(Body::empty()).unwrap();
        let response = fixture.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["rbit-plugin-revision"], revision);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer realm=\"rbit\"");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "Bad credentials");
    }
}

#[tokio::test]
async fn a_disabled_api_keeps_not_found_envelopes_for_rejected_credentials() {
    let fixture = Fixture::with_api(false).await;
    for authorization in [format!("Bearer rbat_{}", "a".repeat(43)), format!("Bearer {}", "a".repeat(4097))] {
        let request =
            Request::builder().uri("/api/v1/repos/alice/rocket").header(header::AUTHORIZATION, authorization).body(Body::empty()).unwrap();
        let response = fixture.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "Not Found");
    }
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn disabling_the_public_api_keeps_runner_protocol_routes_available() {
    let fixture = Fixture::with_api(false).await;
    fixture.services.set_actions_enabled(&fixture.admin, true).await.unwrap();
    let request = Request::builder().method(Method::POST).uri("/api/actions/runner/v1/register").body(Body::empty()).unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    // Missing runner version reaches the RPC's negotiation layer, rather than REST's 404.
    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "unsupported_protocol");
    fixture.services.shutdown().await;
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn one_router_reflects_enable_and_disable_in_pages_navigation_and_openapi() {
    let fixture = Fixture::new().await;
    let repo = fixture.services.repositories.open(Some(&fixture.admin), "alice", "rocket").await.unwrap();
    fixture.database.actions_settings().set_enabled(repo.repo.repo.id, true, rbit_core::Timestamp::now()).await.unwrap();
    let revision = fixture.services.plugin_revision();
    for enabled in [false, true, false] {
        if enabled || fixture.services.actions.is_some() {
            let (status, _, headers) = fixture
                .request(
                    Method::POST,
                    "/admin/plugins/actions",
                    Some(&fixture.admin_cookie),
                    "http://localhost:3000",
                    if enabled { "enabled=true" } else { "enabled=false" },
                )
                .await;
            assert_eq!(status, StatusCode::SEE_OTHER);
            assert_eq!(headers["hx-redirect"], "/admin/plugins");
        }
        let (status, _, _) = fixture.get("/alice/rocket/actions").await;
        assert_eq!(status, if enabled { StatusCode::OK } else { StatusCode::NOT_FOUND });
        let (_, body, _) = fixture.get("/alice/rocket").await;
        assert_eq!(body.contains("data-repo-tab=\"actions\""), enabled);
        let (_, body, _) = fixture.get("/api/v1/openapi.json").await;
        let document: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(document["paths"].as_object().unwrap().keys().any(|path| path.contains("/actions/")), enabled);
        assert_eq!(fixture.services.actions.is_some(), enabled);
    }
    assert!(fixture.services.plugin_revision() > revision);
    assert_eq!(fixture.services.actions_background_tasks(), 0);
    // Stale clients cannot dispatch after the public contribution disappears.
    assert_eq!(
        fixture
            .request(
                Method::POST,
                "/alice/rocket/actions/workflows/build/dispatch",
                Some(&fixture.admin_cookie),
                "http://localhost:3000",
                ""
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[cfg(feature = "i18n")]
fn language_pack(locale: &str, direction: &str, translated: &str) -> Vec<u8> {
    use sha2::{Digest as _, Sha256};
    use std::io::{Cursor, Write as _};
    let text =
        format!("nav-explore = {translated}\nlanguage-pack-count = {{ $count ->\n [one] حزمة واحدة\n *[other] حزم {{ $count }}\n}}\n");
    let license = b"Permission to use.";
    let hex = |bytes: &[u8]| {
        use std::fmt::Write as _;
        bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
            write!(output, "{byte:02x}").unwrap();
            output
        })
    };
    let manifest = format!(
        r#"schema=1
id="rbit.language.{locale}"
version="1.0.0"
locale="{locale}"
direction="{direction}"
message_schema="^1.0"
publisher="Rbit language contributors"
license="MIT"
license_file="LICENSE"
[catalogs]
core="catalogs/core.ftl"
[checksums]
"LICENSE"="{}"
"catalogs/core.ftl"="{}"
"#,
        hex(&Sha256::digest(license)),
        hex(&Sha256::digest(text.as_bytes()))
    );
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).unix_permissions(0o644);
    for (path, bytes) in [("pack.toml", manifest.as_bytes()), ("LICENSE", license.as_slice()), ("catalogs/core.ftl", text.as_bytes())] {
        archive.start_file(path, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn one_router_negotiates_escaped_rtl_messages_and_retains_preferences_across_disable() {
    let fixture = Fixture::new().await;
    fixture.services.install_language_pack(&fixture.admin, &language_pack("ar", "rtl", "<script>alert(1)</script>")).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    fixture.services.set_account_locale(&fixture.admin, Some("ar")).await.unwrap();
    let (status, body, headers) = fixture.get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_LANGUAGE], "ar");
    assert!(body.contains("lang=\"ar\" dir=\"rtl\""));
    assert!(body.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));
    assert!(!body.contains("<script>alert(1)</script>"));
    let vary = headers[header::VARY].to_str().unwrap().to_ascii_lowercase();
    assert!(vary.contains("cookie") && vary.contains("accept-language"));
    assert!(!headers[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
    let old_revision = headers["rbit-locale-revision"].clone();
    let admin_page = fixture.get("/admin/plugins").await.1;
    assert!(admin_page.contains("حزمة واحدة"));
    assert!(admin_page.contains("data-plugin-status=\"i18n\">Enabled</p>"));

    // A mutation carrying the old request snapshot cannot deadlock its own disposal.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        fixture.request(Method::POST, "/admin/plugins/i18n", Some(&fixture.admin_cookie), "http://localhost:3000", "enabled=false"),
    )
    .await
    .unwrap();
    assert_eq!(result.0, StatusCode::SEE_OTHER);
    let (_, body, headers) = fixture.get("/").await;
    assert_eq!(headers[header::CONTENT_LANGUAGE], "en");
    assert_ne!(headers["rbit-locale-revision"], old_revision);
    assert!(!body.contains("href=\"/language\""));
    assert_eq!(fixture.get("/language").await.0, StatusCode::NOT_FOUND);
    assert!(fixture.get("/admin/plugins").await.1.contains("data-plugin-status=\"i18n\">Disabled</p>"));
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    assert_eq!(fixture.get("/").await.2[header::CONTENT_LANGUAGE], "ar");
    fixture.services.remove_language_pack(&fixture.admin, "rbit.language.ar").await.unwrap();
    assert_eq!(fixture.get("/").await.2[header::CONTENT_LANGUAGE], "en");
    fixture.services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn language_preference_and_pack_controls_use_host_authorization_and_origin_checks() {
    let fixture = Fixture::new().await;
    for path in ["/admin/plugins/i18n", "/admin/plugins/i18n/packs/rbit.language.ar/remove"] {
        assert_eq!(
            fixture.request(Method::POST, path, Some(&fixture.user_cookie), "http://localhost:3000", "enabled=true").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture.request(Method::POST, path, Some(&fixture.admin_cookie), "https://attacker.example", "enabled=true").await.0,
            StatusCode::FORBIDDEN
        );
    }
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    assert_eq!(fixture.request(Method::POST, "/language", None, "https://attacker.example", "locale=en").await.0, StatusCode::FORBIDDEN);
    let (status, _, headers) = fixture.request(Method::POST, "/language", None, "http://localhost:3000", "locale=en").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let cookie = headers[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("rbit-locale=en") && cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    fixture.services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn locale_priority_and_bounded_browser_negotiation_are_visible_on_http_responses() {
    let fixture = Fixture::new().await;
    fixture.services.install_language_pack(&fixture.admin, &language_pack("ar", "rtl", "استكشاف")).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    fixture.services.set_account_locale(&fixture.admin, Some("en")).await.unwrap();
    let oversized = "ar,".repeat(1400);
    for (cookie, accept, expected) in [
        (Some(format!("{}; rbit-locale=ar", fixture.admin_cookie)), "ar", "en"),
        (Some("rbit-locale=en".to_owned()), "ar", "en"),
        (Some("rbit-locale=ar".to_owned()), "en", "ar"),
        (None, "ar;q=0.9,en;q=0.5", "ar"),
        (None, oversized.as_str(), "en"),
    ] {
        let mut request = Request::builder().uri("/").header(header::ACCEPT_LANGUAGE, accept);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = fixture.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.headers()[header::CONTENT_LANGUAGE], expected);
    }
    fixture.services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn trusted_pack_upload_and_removal_publish_without_rebinding_the_router() {
    let fixture = Fixture::new().await;
    let pack = language_pack("ar", "rtl", "استكشاف");
    let mut multipart =
        b"--rbitpack\r\nContent-Disposition: form-data; name=\"pack\"; filename=\"ar.zip\"\r\nContent-Type: application/zip\r\n\r\n"
            .to_vec();
    multipart.extend_from_slice(&pack);
    multipart.extend_from_slice(b"\r\n--rbitpack--\r\n");
    for (cookie, origin, expected) in [
        (&fixture.user_cookie, "http://localhost:3000", StatusCode::NOT_FOUND),
        (&fixture.admin_cookie, "https://attacker.example", StatusCode::FORBIDDEN),
        (&fixture.admin_cookie, "http://localhost:3000", StatusCode::SEE_OTHER),
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/admin/plugins/i18n/packs")
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, origin)
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "multipart/form-data; boundary=rbitpack")
            .body(Body::from(multipart.clone()))
            .unwrap();
        let response = fixture.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
    }
    assert!(fixture.get("/admin/plugins").await.1.contains("rbit.language.ar"));
    let status = fixture
        .request(
            Method::POST,
            "/admin/plugins/i18n/packs/rbit.language.ar/remove",
            Some(&fixture.admin_cookie),
            "http://localhost:3000",
            "",
        )
        .await
        .0;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!fixture.get("/admin/plugins").await.1.contains("rbit.language.ar"));
    fixture.services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn failed_language_startup_is_visible_and_repairable_in_the_admin_page() {
    use rbit_core::Timestamp;
    use rbit_storage::sqlite::language_packs::NewLanguagePack;

    let mut fixture = Fixture::new().await;
    fixture
        .database
        .language_packs()
        .put(&NewLanguagePack {
            id: "example.broken",
            locale: "fr",
            version: "1.0.0",
            bytes: b"private-corrupt-archive-data",
            digest: &"0".repeat(64),
            message_count: 1,
            message_total: rbit_messages::MessageSchema::builtin().len(),
            installed_by: Some(fixture.admin.id()),
            updated_at: Timestamp::now(),
        })
        .await
        .unwrap();
    fixture.database.plugin_settings().set_desired_enabled("rbit.i18n", true, Some(fixture.admin.id()), Timestamp::now()).await.unwrap();
    fixture.services.shutdown().await;
    let resumed = Services::new(Arc::clone(&fixture.config), &fixture.database, &fixture.kv).await.unwrap();
    fixture.router = rbit_web::router(rbit_web::AppState::new(Arc::clone(&fixture.config), resumed.clone()));
    fixture.services = resumed;
    let (status, body, headers) = fixture.get("/admin/plugins").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_LANGUAGE], "en");
    assert!(body.contains("data-plugin-status=\"i18n\">Failed</p>"));
    assert!(body.contains("English remains available."));
    assert!(body.contains("example.broken"));
    assert!(body.contains("/admin/plugins/i18n/packs/example.broken/remove"));
    assert!(!body.contains("private-corrupt-archive-data"));
    assert!(!fixture.services.i18n_enabled());

    let (status, _, _) =
        fixture.request(Method::POST, "/admin/plugins/i18n", Some(&fixture.admin_cookie), "http://localhost:3000", "enabled=false").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(fixture.get("/admin/plugins").await.1.contains("data-plugin-status=\"i18n\">Disabled</p>"));
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn runtime_ui_registration_renders_through_askama_and_cleans_up_on_one_router() {
    use rbit_plugin_api::{
        Generation, UiContent, UiContribution, UiContributionSet, UiIcon, UiLink, UiMessage, UiNoticeTone, UiSlot, UiVisibility,
    };
    use rbit_plugin_runtime::PluginScope;

    let fixture = Fixture::new().await;
    let registry = fixture.services.ui_registry();
    let scope = PluginScope::new(Generation::INITIAL);
    let contribution = |id: &str, slot, visibility, label: &str, target: &str| UiContribution {
        id: id.parse().unwrap(),
        slot,
        order: 0,
        visibility,
        content: UiContent::Link(UiLink {
            label: UiMessage { namespace: "example.ui".into(), key: "label".into(), fallback: label.into() },
            target: target.parse().unwrap(),
            icon: UiIcon::Info,
        }),
    };
    let set = UiContributionSet::new(vec![
        contribution("navigation", UiSlot::GlobalNav, UiVisibility::Public, "<script>Example</script>", "/explore"),
        contribution("account", UiSlot::AccountSettings, UiVisibility::SignedIn, "Example account", "/settings/example"),
        contribution("admin", UiSlot::AdminNav, UiVisibility::InstanceAdmin, "Example administrator", "/admin/example"),
        UiContribution {
            id: "panel".parse().unwrap(),
            slot: UiSlot::RepositoryPanel,
            order: 0,
            visibility: UiVisibility::RepositoryReader,
            content: UiContent::Notice {
                message: UiMessage { namespace: "example.ui".into(), key: "notice".into(), fallback: "Example repository panel".into() },
                tone: UiNoticeTone::Info,
            },
        },
    ]);
    let registration = registry.stage("example.ui".parse().unwrap(), &scope, scope.generation(), set).unwrap();
    assert!(!fixture.get("/admin/plugins").await.1.contains("data-plugin-ui=\"example.ui:"));
    registration.publish().unwrap();
    let (_, html, headers) = fixture.get("/admin/plugins").await;
    assert!(html.contains("data-plugin-ui=\"example.ui:admin\""));
    assert!(html.contains("Example administrator"));
    assert!(!html.contains("<script>Example</script>"));
    assert!(html.contains("Example&#60;/script&#62;") || html.contains("Example&lt;/script&gt;"));
    assert!(fixture.get("/settings/profile").await.1.contains("Example account"));
    let response = fixture
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/alice/rocket")
                .header("Rbit-Partial", "/alice/rocket")
                .header(header::COOKIE, &fixture.admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["rbit-partial"], "true");
    let partial = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert_eq!(partial.matches("id=\"repo-plugin-panels\"").count(), 1);
    assert!(partial.contains("Example repository panel"));
    assert!(partial.contains("hx-swap-oob=\"true\""));
    let user_page = fixture.request(Method::GET, "/settings/profile", Some(&fixture.user_cookie), "http://localhost:3000", "").await.1;
    assert!(!user_page.contains("Example administrator"));
    assert!(user_page.contains("Example account"));
    assert_eq!(fixture.get("/admin/example").await.0, StatusCode::NOT_FOUND, "UI registration grants no route or authorization");

    registration
        .replace(UiContributionSet::new(vec![contribution(
            "navigation",
            UiSlot::GlobalNav,
            UiVisibility::Public,
            "Replacement UI",
            "/explore",
        )]))
        .unwrap();
    let (_, html, replacement_headers) = fixture.get("/admin/plugins").await;
    assert!(html.contains("Replacement UI"));
    assert!(!html.contains("Example administrator"));
    assert_ne!(headers["rbit-plugin-revision"], replacement_headers["rbit-plugin-revision"]);
    scope.quiesce();
    let (status, html, _) = fixture.get("/admin/plugins").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("data-plugin-ui=\"example.ui:"));
    assert!(registration.publish().is_err());
    assert_eq!(scope.in_flight(), 0, "render snapshots do not hold plugin operations open");
    fixture.services.shutdown().await;
}
