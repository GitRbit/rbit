//! Real Git history, visibility, attribution and rendered contribution pages.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateOrganization, CreateRepository, Services};
use rbit_storage::{Database, KvStore, NewUser};
use std::{path::Path, process::Command, sync::Arc};
use tower::ServiceExt;

#[test]
fn responsive_panels_cover_the_leap_year_without_horizontal_overflow() {
    let calendar = rbit_web::views::contributions::ContributionCalendar::new(Some(2024), "test", "")
        .populate(&rbit_services::contributions::Contributions::default());
    let days: Vec<_> = calendar.quarters.iter().flat_map(|q| &q.days).collect();
    assert_eq!(days.len(), 366);
    let unique: std::collections::HashSet<_> = days.iter().map(|day| &day.date).collect();
    assert_eq!(unique.len(), 366);
    assert!(days.iter().all(|day| day.x + 12 <= 246 && day.y + 12 <= 133));
    assert_eq!(calendar.quarters.iter().map(|q| q.months.len()).sum::<usize>(), 12);
}

fn git(path: &Path, args: &[&str], email: &str, date: &str) {
    let output = Command::new("git")
        .current_dir(path)
        .args(["-c", "user.name=Contributor", "-c", &format!("user.email={email}"), "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn contribution_counts_respect_visibility_authors_branches_and_years() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("data");
    config.security.rate_limit = false;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let config = Arc::new(config);
    let services = Services::new(config.clone(), &database, &kv).await.unwrap();
    let alice = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: false,
        })
        .await
        .unwrap();
    let org = services.orgs.create(&alice, CreateOrganization { name: "acme".into(), ..Default::default() }).await.unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let run = |args: &[&str], email: &str, date: &str| git(&source, args, email, date);
    run(&["init", "-q"], "alice@example.com", "2024-02-29T12:00:00Z");
    run(&["commit", "--allow-empty", "-qm", "leap day"], "ALICE@example.com", "2024-02-29T12:00:00Z");
    run(&["commit", "--allow-empty", "-qm", "another author"], "bob@example.com", "2024-03-01T12:00:00Z");
    run(&["commit", "--allow-empty", "-qm", "next year"], "alice@example.com", "2025-01-01T12:00:00Z");
    // Deliberately non-monotonic author dates must not stop the walk prematurely.
    run(&["commit", "--allow-empty", "-qm", "backdated"], "alice@example.com", "2024-01-01T12:00:00Z");
    for (owner, name, visibility) in
        [("acme", "public", Visibility::Public), ("alice", "duplicate", Visibility::Public), ("alice", "private", Visibility::Private)]
    {
        let repo = services
            .repositories
            .create_in(&alice, owner, CreateRepository { name: name.into(), description: String::new(), visibility })
            .await
            .unwrap();
        let path = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(repo.owner.id, repo.repo.id);
        git(&path, &["fetch", "-q", source.to_str().unwrap(), "main:main"], "alice@example.com", "2024-01-01T12:00:00Z");
    }
    let empty = services
        .repositories
        .create(&alice, CreateRepository { name: "empty".into(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    let private = services.repositories.open(Some(&alice), "alice", "private").await.unwrap();
    run(&["commit", "--allow-empty", "-qm", "private only"], "alice@example.com", "2024-12-01T12:00:00Z");
    git(&private.disk_path, &["fetch", "-q", source.to_str().unwrap(), "main:main"], "alice@example.com", "2024-12-01T12:00:00Z");
    // A branch that has not reached the default branch is excluded.
    run(&["checkout", "-qb", "side"], "alice@example.com", "2024-01-01T12:00:00Z");
    run(&["commit", "--allow-empty", "-qm", "side only"], "alice@example.com", "2024-04-01T12:00:00Z");
    git(&private.disk_path, &["fetch", "-q", source.to_str().unwrap(), "side:side"], "alice@example.com", "2024-01-01T12:00:00Z");
    let start = 1_704_067_200; // 2024-01-01 UTC
    let end = 1_735_689_600; // 2025-01-01 UTC
    // A genuinely cold initial page must leave the expensive contribution cache empty.
    let cold_router = rbit_web::router(rbit_web::AppState::new(config.clone(), services.clone()));
    for url in ["/acme/public/activity", "/alice", "/acme"] {
        let response = cold_router.clone().oneshot(Request::builder().uri(url).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap();
        assert!(String::from_utf8(body.to_vec()).unwrap().contains("calendar=fragment"));
    }
    assert_eq!(kv.insights().entry_count().await.unwrap(), 0, "initial pages must not scan history for their calendars");
    // Populate the shared history cache as a privileged viewer first.
    let own = services.contributions.account(Some(&alice), &alice.account, start, end).await.unwrap();
    assert_eq!(own.days.values().sum::<usize>(), 3);
    let public = services.contributions.account(None, &alice.account, start, end).await.unwrap();
    assert_eq!(public.days.values().sum::<usize>(), 2, "private activity must not leak through the cache; forks count once");
    assert_eq!(public.repositories, 2, "both visible repositories are represented even when their commits overlap");
    let organization = services.contributions.account(None, &org.account, start, end).await.unwrap();
    assert_eq!(organization.days.values().sum::<usize>(), 3, "organization counts all authors");
    assert!(!organization.partial);
    run(&["checkout", "-q", "main"], "alice@example.com", "2024-12-02T12:00:00Z");
    run(&["commit", "--allow-empty", "-qm", "new private tip"], "alice@example.com", "2024-12-02T12:00:00Z");
    git(&private.disk_path, &["fetch", "-q", source.to_str().unwrap(), "main:main"], "alice@example.com", "2024-12-02T12:00:00Z");
    let updated = services.contributions.repository(&private, start, end).await.unwrap();
    assert_eq!(updated.days.values().sum::<usize>(), 5, "a new branch tip must bypass the old cache entry");
    let empty = services.repositories.open(None, "alice", empty.repo.name.as_str()).await.unwrap();
    assert!(services.contributions.repository(&empty, start, end).await.unwrap().days.is_empty());
    let router = rbit_web::router(rbit_web::AppState::new(config, services));
    assert_contribution_pages(router).await;
}

async fn assert_contribution_pages(router: axum::Router) {
    for (url, expected) in [
        ("/alice?year=2024", "2 commits in 2024"),
        ("/alice?year=2025&tab=stars", "1 commit in 2025"),
        ("/acme?year=2024", "3 commits in 2024"),
        ("/acme/public/activity?year=2024", "3 commits in 2024"),
        ("/alice/empty/activity?year=2024", "No matching commits in this year."),
    ] {
        let response =
            router.clone().oneshot(Request::builder().uri(format!("{url}&calendar=1")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{url}");
        let body = axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains(expected), "{url}: expected {expected}");
        assert!(html.contains("Commit activity in"));
        assert!(html.contains("Commit calendar by quarter"));
        assert!(!html.contains("Scrollable commit calendar"));
        assert!(!html.contains("ALICE@example.com"), "author email is never rendered");
        if url.contains("/activity?") {
            assert!(html.contains("data-repo-tab=\"activity\""));
            assert!(
                html.contains("href=\"/acme/public/activity\" aria-current=\"page\"")
                    || html.contains("href=\"/alice/empty/activity\" aria-current=\"page\"")
            );
        }
    }
    // Initial navigation must not wait for any calendar history scan or send its SVG.
    for url in ["/alice?year=2024", "/acme?year=2024", "/acme/public/activity?year=2024", "/alice/empty/activity?year=2024"] {
        let response = router.clone().oneshot(Request::builder().uri(url).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap().to_vec()).unwrap();
        assert!(html.contains("calendar=fragment"));
        assert!(html.contains("calendar=1"), "ordinary link supports JavaScript-free browsing");
        assert!(!html.contains("data-contribution-day=\""), "{url}: initial response must omit calendar cells");
        let response = router
            .clone()
            .oneshot(Request::builder().uri(format!("{url}&calendar=fragment")).header("HX-Request", "true").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap().to_vec()).unwrap();
        assert!(html.contains("Commit activity in 2024"));
        assert!(!html.contains("<main") && !html.contains("<html"));
    }
    for url in [
        "/acme/public",
        "/alice/empty",
        "/acme/public/commits",
        "/acme/public/commits/main?year=2024&page=2&calendar=fragment",
        "/alice/empty/commits?year=2024&calendar=1",
    ] {
        let response = router.clone().oneshot(Request::builder().uri(url).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(!html.contains("id=\"contributions-title\"") && !html.contains("calendar=fragment"), "{url}: heatmap belongs on Activity");
        assert!(html.contains("data-repo-tab=\"activity\""));
        if url.contains("page=2") {
            assert!(html.contains("/commits/main?page=1"), "commit pagination is independent of calendar years");
            assert!(html.contains("href=\"/acme/public/commits\" aria-current=\"page\""));
        } else if url == "/acme/public/commits" {
            assert!(html.contains("backdated") && html.contains("leap day"), "commit history still renders");
        }
    }
    let response =
        router.clone().oneshot(Request::builder().uri("/alice/private/activity?year=2024").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = router
        .oneshot(Request::builder().uri("/alice/private/activity?year=2024&calendar=fragment").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
