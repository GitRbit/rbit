//! Repository document discovery and selection through the real router, Git and templates.
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services};
use rbit_storage::{Database, KvStore, NewUser};
use tower::ServiceExt;

const HOME: &str = "/docs-owner/public";

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["-c", "user.name=Docs Test", "-c", "user.email=docs@example.com", "-c", "init.defaultBranch=main"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write_documents(source: &Path) {
    for directory in [".github/assets", "docs/images"] {
        std::fs::create_dir_all(source.join(directory)).unwrap();
    }
    for (path, content) in [
        ("README.md", "# Root documentation marker\n\n[Guide](docs/guide.md)\n"),
        ("README.txt", "Alternate README marker\n"),
        ("LICENSE-MIT", "MIT license body marker\n<script>unsafe-license-marker()</script>\n"),
        ("LICENSE-APACHE", "Apache license body marker\n"),
        (
            ".github/CODE_OF_CONDUCT.md",
            "# Community standards marker\n\n[Contact](../docs/contact.md)\n![Badge](assets/badge.svg)\n<script>unsafe-markdown-marker()</script>\n",
        ),
        ("docs/CONTRIBUTING.md", "# Contribution instructions marker\n\n[Guide](guide.md)\n![Badge](images/badge.svg)\n"),
        ("docs/README.md", "# Nested README marker\n\n[Guide](guide.md)\n"),
        ("docs/guide.md", "A guide."),
        ("docs/contact.md", "A contact."),
        ("secret.txt", "UNSELECTABLE_FILE_BODY_MARKER"),
    ] {
        std::fs::write(source.join(path), content).unwrap();
    }
    std::fs::write(source.join(".github/SECURITY.md"), b"\0UNRENDERABLE_BINARY_BODY_MARKER").unwrap();
    std::fs::write(source.join("NOTICE"), "OVERSIZE_DOCUMENT_BODY_MARKER".repeat(200)).unwrap();
}

struct Fixture {
    _directory: tempfile::TempDir,
    _kv: KvStore,
    source: PathBuf,
    bare: PathBuf,
    router: Router,
    cookie: String,
    main_commit: String,
    branch_commit: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.repository.max_render_bytes = 1024;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let owner = database
            .accounts()
            .create_user(NewUser {
                name: "docs-owner".parse().unwrap(),
                email: "docs@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "unused by session-only fixture".into(),
                is_admin: false,
            })
            .await
            .unwrap();
        let session = services.accounts.start_session(&owner).await.unwrap();
        let cookie = format!("rbit_session={}", session.encode());
        let source = directory.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-q"]);
        write_documents(&source);
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "Initial documentation"]);
        let main_commit = git(&source, &["rev-parse", "HEAD"]);
        git(&source, &["checkout", "-qb", "docs-next"]);
        std::fs::write(source.join("README.md"), "# Nondefault branch marker\n\n[Guide](docs/guide.md)\n").unwrap();
        git(&source, &["commit", "-qam", "Branch documentation"]);
        let branch_commit = git(&source, &["rev-parse", "HEAD"]);
        let mut bare = PathBuf::new();
        for (name, visibility) in [("public", Visibility::Public), ("private", Visibility::Private)] {
            let repo = services
                .repositories
                .create(&owner, CreateRepository { name: name.into(), description: String::new(), visibility })
                .await
                .unwrap();
            let target = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(repo.owner.id, repo.repo.id);
            git(&target, &["fetch", "-q", source.to_str().unwrap(), "main:main", "docs-next:docs-next"]);
            if name == "public" {
                bare = target;
            }
        }
        let router = rbit_web::router(rbit_web::AppState::new(config, services));
        Self { _directory: directory, _kv: kv, source, bare, router, cookie, main_commit, branch_commit }
    }

    async fn get(&self, path: &str, signed_in: bool, partial: bool) -> (StatusCode, String) {
        let mut request = Request::builder().uri(path);
        if signed_in {
            request = request.header(header::COOKIE, &self.cookie);
        }
        if partial {
            request = request.header("HX-Request", "true").header("Rbit-Partial", "/docs-owner/private");
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let content = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        (status, content)
    }

    async fn document(&self, page: &str, document: &str) -> (StatusCode, String) {
        let document = utf8_percent_encode(document, NON_ALPHANUMERIC);
        self.get(&format!("{page}?document={document}"), false, false).await
    }
}

#[tokio::test]
async fn home_and_tree_select_readmes_and_multiple_licenses_without_javascript() {
    let fixture = Fixture::new().await;
    let (status, home) = fixture.get(HOME, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(home.contains("Root documentation marker"));
    let decoded = percent_decode_str(&home).decode_utf8_lossy();
    for path in ["README.md", "README.txt", "LICENSE-MIT", "LICENSE-APACHE", ".github/CODE_OF_CONDUCT.md", "docs/CONTRIBUTING.md"] {
        assert!(decoded.contains(&format!("document={path}")), "missing document link for {path}");
    }
    let tree = format!("{HOME}/tree/{}", fixture.main_commit);
    for page in [HOME, tree.as_str()] {
        let (status, license) = fixture.document(page, "LICENSE-MIT").await;
        assert_eq!(status, StatusCode::OK);
        assert!(license.contains("MIT license body marker"));
        assert!(!license.contains("<script>unsafe-license-marker()"));
        assert!(license.contains("&#60;script&#62;") || license.contains("&lt;script&gt;"));
        let (status, license) = fixture.document(page, "LICENSE-APACHE").await;
        assert_eq!(status, StatusCode::OK);
        assert!(license.contains("Apache license body marker"));
        let (status, readme) = fixture.document(page, "README.txt").await;
        assert_eq!(status, StatusCode::OK);
        assert!(readme.contains("Alternate README marker"));
    }
}

#[tokio::test]
async fn fallback_and_nested_documents_resolve_links_from_their_actual_directories() {
    let fixture = Fixture::new().await;
    let (status, conduct) = fixture.document(HOME, ".github/CODE_OF_CONDUCT.md").await;
    assert_eq!(status, StatusCode::OK);
    assert!(conduct.contains("Community standards marker"));
    assert!(conduct.contains(&format!("/blob/{}/docs/contact.md", fixture.main_commit)));
    assert!(conduct.contains(&format!("/raw/{}/.github/assets/badge.svg", fixture.main_commit)));
    assert!(!conduct.contains("unsafe-markdown-marker()"));
    let (status, contributing) = fixture.document(HOME, "docs/CONTRIBUTING.md").await;
    assert_eq!(status, StatusCode::OK);
    assert!(contributing.contains("Contribution instructions marker"));
    assert!(contributing.contains(&format!("/blob/{}/docs/guide.md", fixture.main_commit)));
    assert!(contributing.contains(&format!("/raw/{}/docs/images/badge.svg", fixture.main_commit)));
    let nested = format!("{HOME}/tree/{}/docs", fixture.main_commit);
    let (status, content) = fixture.get(&nested, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content.contains("Nested README marker"));
    assert!(content.contains(&format!("/blob/{}/docs/guide.md", fixture.main_commit)));
}

#[tokio::test]
async fn document_selection_respects_whitelists_and_private_repository_access() {
    let fixture = Fixture::new().await;
    for document in ["does-not-exist.md", "secret.txt", "../README.md", "/README.md", "docs/../secret.txt", "README.md/../secret.txt"] {
        let (status, content) = fixture.document(HOME, document).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "selection {document}");
        assert!(!content.contains("UNSELECTABLE_FILE_BODY_MARKER"));
    }
    for partial in [false, true] {
        let path = "/docs-owner/private?document=LICENSE-MIT";
        let (status, content) = fixture.get(path, false, partial).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!content.contains("MIT license body marker"));
        assert!(!content.contains("Root documentation marker"));
        let (status, content) = fixture.get(path, true, partial).await;
        assert_eq!(status, StatusCode::OK);
        assert!(content.contains("MIT license body marker"));
    }
}

#[tokio::test]
async fn binary_and_oversize_documents_have_safe_bounded_fallbacks() {
    let fixture = Fixture::new().await;
    for (path, message, body_marker) in [
        (
            ".github/SECURITY.md",
            "This document is binary or is not valid UTF-8 and cannot be displayed.",
            "UNRENDERABLE_BINARY_BODY_MARKER",
        ),
        ("NOTICE", "This document is too large to display.", "OVERSIZE_DOCUMENT_BODY_MARKER"),
    ] {
        let (status, content) = fixture.document(HOME, path).await;
        assert_eq!(status, StatusCode::OK);
        assert!(content.contains(message));
        assert!(!content.contains(body_marker));
    }
}

#[tokio::test]
async fn nondefault_revision_documents_and_switch_links_remain_pinned_after_branch_updates() {
    let fixture = Fixture::new().await;
    let branch = format!("{HOME}/tree/docs-next");
    let (status, content) = fixture.get(&branch, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content.contains("Nondefault branch marker"));
    assert!(content.contains(&format!("/tree/{}?", fixture.branch_commit)));
    assert!(content.contains(&format!("/blob/{}/docs/guide.md", fixture.branch_commit)));
    std::fs::write(fixture.source.join("README.md"), "# Updated branch tip marker\n").unwrap();
    git(&fixture.source, &["commit", "-qam", "Advance documentation"]);
    git(&fixture.bare, &["fetch", "-q", fixture.source.to_str().unwrap(), "docs-next:docs-next"]);
    let (status, pinned) = fixture.document(&format!("{HOME}/tree/{}", fixture.branch_commit), "README.md").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pinned.contains("Nondefault branch marker"));
    assert!(!pinned.contains("Updated branch tip marker"));
    let (status, current) = fixture.get(&branch, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(current.contains("Updated branch tip marker"));
}

#[tokio::test]
async fn document_query_preserves_numeric_pagination_and_search_parsing() {
    let fixture = Fixture::new().await;
    for page in [HOME.to_owned(), format!("{HOME}/tree/{}", fixture.main_commit)] {
        let (status, content) = fixture.get(&format!("{page}?document=LICENSE-MIT&page=2&q=LICENSE"), false, false).await;
        assert_eq!(status, StatusCode::OK, "numeric pagination must deserialize on {page}");
        assert!(content.contains("MIT license body marker"));
        assert!(!content.contains("UNSELECTABLE_FILE_BODY_MARKER"));
    }
}
