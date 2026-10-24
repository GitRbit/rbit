//! Archived repositories through the real router, database and templates: the banner,
//! the hidden write controls, the list badges and the explore filter.
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use rbit_core::{Config, User, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt;

const BANNER: &str = "This repository has been archived.";

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
    router: axum::Router,
    owner: User,
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
        let signup = SignUp { username: "keeper".into(), email: "keeper@example.com".into(), password: "correct horse battery".into() };
        let owner = services.accounts.create_user(signup, Some(false)).await.unwrap();
        for name in ["frozen", "lively"] {
            let input = CreateRepository { name: name.into(), description: format!("The {name} one"), visibility: Visibility::Public };
            services.repositories.create(&owner, input).await.unwrap();
        }
        let frozen = services.repositories.open(Some(&owner), "keeper", "frozen").await.unwrap();
        services.repo_settings.set_archived(&owner, &frozen, true).await.unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _dir: dir, services, router, owner }
    }

    async fn get(&self, path: &str) -> (StatusCode, String) {
        let response = self.router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }
}

/// The names of the repositories an explore page lists, in order.
fn listed(html: &str) -> Vec<&str> {
    ["frozen", "lively"].into_iter().filter(|name| html.contains(&format!("href=\"/keeper/{name}\""))).collect()
}

#[tokio::test]
async fn archived_repositories_show_a_banner_and_no_write_controls() {
    let fixture = Fixture::new().await;
    for path in ["/keeper/frozen", "/keeper/frozen/issues", "/keeper/frozen/pulls"] {
        let (status, html) = fixture.get(path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(html.contains(BANNER), "{path} shows the banner");
    }
    let (_, issues) = fixture.get("/keeper/frozen/issues").await;
    assert!(!issues.contains(">New issue<"), "no way to open an issue");
    let (_, pulls) = fixture.get("/keeper/frozen/pulls").await;
    assert!(!pulls.contains("/keeper/frozen/compare"), "no way to open a pull request");

    let (_, active) = fixture.get("/keeper/lively/issues").await;
    assert!(!active.contains(BANNER));
    assert!(active.contains(">New issue<"));

    let frozen = fixture.services.repositories.open(Some(&fixture.owner), "keeper", "frozen").await.unwrap();
    fixture.services.repo_settings.set_archived(&fixture.owner, &frozen, false).await.unwrap();
    let (_, unarchived) = fixture.get("/keeper/frozen/issues").await;
    assert!(!unarchived.contains(BANNER) && unarchived.contains(">New issue<"), "unarchiving brings the controls back");
}

#[tokio::test]
async fn lists_mark_archived_repositories_and_explore_filters_them() {
    let fixture = Fixture::new().await;
    let (_, all) = fixture.get("/explore").await;
    assert_eq!(listed(&all), ["frozen", "lively"]);
    assert_eq!(all.matches(">Archived</span>").count(), 1, "one badge, on the archived repository");
    assert!(all.contains("href=\"/explore?archived=true\"") && all.contains("href=\"/explore?archived=false\""));

    let (_, active) = fixture.get("/explore?archived=false").await;
    assert_eq!(listed(&active), ["lively"]);
    assert!(active.contains("1 repository"));
    assert!(active.contains("name=\"archived\" value=\"false\""), "searching keeps the filter");
    let (_, archived) = fixture.get("/explore?archived=true&q=one").await;
    assert_eq!(listed(&archived), ["frozen"]);
    let (_, lenient) = fixture.get("/explore?archived=maybe").await;
    assert_eq!(listed(&lenient), ["frozen", "lively"], "an unknown value lists everything");

    let (_, profile) = fixture.get("/keeper").await;
    assert_eq!(profile.matches(">Archived</span>").count(), 1, "profiles mark it too");
}
