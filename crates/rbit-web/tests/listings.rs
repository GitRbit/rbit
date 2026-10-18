//! Bounded pages tested through the real router, database, Git reader and templates.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};
use tower::ServiceExt;

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one fixture, checked listing by listing")]
async fn listings_page_search_pin_metadata_and_preserve_private_access() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().join("data");
    config.security.rate_limit = false;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let db = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let config = Arc::new(config);
    let services = Services::new(Arc::clone(&config), &db, &kv).await.unwrap();
    let owner = services
        .accounts
        .create_user(
            SignUp { username: "listing-owner".into(), email: "listing@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "-q"]);
    for i in 0..205 {
        std::fs::write(source.join(format!("file-{i:03}.rs")), "fn main() {}\n").unwrap();
    }
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "initial"]);
    let head = git(&source, &["rev-parse", "HEAD"]);
    for (name, visibility) in [("public", Visibility::Public), ("private", Visibility::Private)] {
        let repository = services
            .repositories
            .create(&owner, CreateRepository { name: name.into(), description: String::new(), visibility })
            .await
            .unwrap();
        let bare = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(repository.owner.id, repository.repo.id);
        git(&bare, &["fetch", "-q", source.to_str().unwrap(), "main:main"]);
        let mut child = Command::new("git")
            .current_dir(&bare)
            .args(["update-ref", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        {
            let input = child.stdin.as_mut().unwrap();
            for i in 0..105 {
                writeln!(input, "update refs/tags/v{i:03} {head}").unwrap();
            }
        }
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
    }
    let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
    let get = |path: String| {
        let router = router.clone();
        async move {
            let response = router.oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            (status, String::from_utf8(body.to_vec()).unwrap())
        }
    };
    let (_, first) = get("/listing-owner/public".into()).await;
    assert_eq!(first.matches("id=\"entry-commit-").count(), 100);
    assert!(first.contains(&format!("/tree/{head}?q=")), "next page is pinned");
    let (_, second) = get(format!("/listing-owner/public/tree/{head}?page=2")).await;
    assert_eq!(second.matches("id=\"entry-commit-").count(), 100);
    assert!(second.contains("id=\"entry-commit-100\""));
    assert!(!second.contains("id=\"entry-commit-0\""));
    let (_, last) = get(format!("/listing-owner/public/tree/{head}?page=9999")).await;
    assert_eq!(last.matches("id=\"entry-commit-").count(), 5);
    let (_, search) = get(format!("/listing-owner/public/tree/{head}?q=FILE-204")).await;
    assert_eq!(search.matches("id=\"entry-commit-").count(), 1);
    assert!(search.contains("id=\"entry-commit-204\""));
    let (_, cells) = get(format!("/listing-owner/public/tree-commits/{head}?page=2")).await;
    assert_eq!(cells.matches("id=\"entry-commit-").count(), 100);
    assert!(cells.contains("id=\"entry-commit-100\""));
    assert!(!cells.contains("id=\"entry-commit-0\""));
    let context = services.repositories.open(None, "listing-owner", "public").await.unwrap();
    let tree = context.reader.tree(head.clone(), String::new()).await.unwrap();
    assert_eq!(
        services.insights.last_commits(&context, &head, "", &tree.entries).await.unwrap().len(),
        205,
        "a page must not poison the whole-directory cache"
    );
    let (_, tags) = get("/listing-owner/public/tags".into()).await;
    assert_eq!(tags.matches("title=\"Download ZIP\"").count(), 100);
    let (_, tags) = get("/listing-owner/public/tags?page=2".into()).await;
    assert_eq!(tags.matches("title=\"Download ZIP\"").count(), 5);
    let (_, tags) = get("/listing-owner/public/tags?q=V104".into()).await;
    assert_eq!(tags.matches("title=\"Download ZIP\"").count(), 1);
    for (query, count) in [("", 100), ("&page=3", 5), ("&q=FILE-204", 1)] {
        let (status, files) = get(format!("/listing-owner/public/language-files?revision={head}&language=Rust{query}")).await;
        assert_eq!(status, StatusCode::OK);
        // The dialog's folder outline is built from this page's paths only.
        assert_eq!(files.matches("data-outline-file").count(), count);
        assert!(files.contains("data-file-outline"));
    }
    let (status, graph) = get("/listing-owner/public/branches?view=tree".into()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(graph.contains("Branch commit graph"));
    for suffix in [
        "tags".to_owned(),
        "branches?view=tree".into(),
        format!("tree/{head}?page=2"),
        format!("tree-commits/{head}?page=2"),
        format!("language-files?revision={head}&language=Rust"),
    ] {
        assert_eq!(get(format!("/listing-owner/private/{suffix}")).await.0, StatusCode::NOT_FOUND);
    }
}
