//! The Actions settings pages render their text through the catalog: a Spanish page shows
//! the translated messages without English marking, and an English page wraps nothing.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::Config;
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    router: Router,
    owner: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.actions.enabled = true;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let sign_up = SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() };
        let alice = services.accounts.create_user(sign_up, Some(true)).await.unwrap();
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let owner = format!("rbit_session={}", services.accounts.start_session(&alice).await.unwrap().encode());
        let router = rbit_web::router(AppState::new(config, services));
        Self { _directory: directory, router, owner }
    }

    /// Turns Actions on for the repository, as its settings form does; environments need it.
    async fn enable(&self) {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/alice/rocket/settings/actions")
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::COOKIE, &self.owner)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("enabled=on&token_permission=read"))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_redirection(), "Actions turned on: {}", response.status());
    }

    /// The page's main content, as the owner sees it in `language`.
    async fn main(&self, path: &str, language: &str) -> String {
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "localhost:3000")
            .header(header::COOKIE, &self.owner)
            .header(header::ACCEPT_LANGUAGE, language)
            .body(Body::empty())
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        body.split("<main id=\"main\"").nth(1).unwrap().to_owned()
    }
}

#[tokio::test]
async fn settings_text_is_translated_in_spanish() {
    let fixture = Fixture::new().await;
    fixture.enable().await;
    let pages = [
        (
            "/alice/rocket/settings/actions",
            [
                ("Run workflows in this repository", "Ejecutar flujos de trabajo en este repositorio"),
                ("Trusted pull request authors", "Autores de confianza de solicitudes de cambios"),
            ],
        ),
        (
            "/alice/rocket/settings/actions/runners",
            [("Registration tokens", "Tokens de registro"), ("No runners yet", "Aún no hay agentes")],
        ),
        ("/alice/rocket/settings/actions/environments", [("New environment", "Nuevo entorno"), ("No environments", "No hay entornos")]),
        ("/settings/actions", [("Add secret", "Añadir secreto"), ("No variables", "No hay variables")]),
    ];
    for (path, messages) in pages {
        let spanish = fixture.main(path, "es").await;
        assert!(spanish.contains("lang=\"es\""), "{path} content follows the page language");
        for (english, translated) in messages {
            assert!(spanish.contains(translated), "{path}: {translated} is translated: {spanish}");
            assert!(!spanish.contains(english), "{path}: {english} no longer falls back");
        }
        assert!(!spanish.contains("<span lang=\"en\">"), "{path}: nothing falls back to English");
        let english = fixture.main(path, "en").await;
        assert!(messages.iter().all(|(text, _)| english.contains(text)), "{path} in English");
        assert!(!english.contains("<span lang=\"en\">"), "{path}: English pages never wrap their own text");
    }
    let spanish = fixture.main("/alice/rocket/settings/actions", "es").await;
    assert!(spanish.contains(">Configuración</a>"), "the shell's translated breadcrumb is not wrapped");
    assert!(
        spanish.contains("Los flujos de trabajo de <span class=\"font-mono text-ink\" dir=\"ltr\">"),
        "a styled argument stays inside its translated sentence"
    );
}
