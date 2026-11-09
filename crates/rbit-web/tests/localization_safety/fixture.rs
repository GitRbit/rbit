//! A public repository with hostile text in its description, an issue and a pull request,
//! served through the real router with the diagnostic catalogs enabled.
use std::{path::Path, process::Command, sync::Arc};

use axum::{
    Extension, Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use rbit_core::{Config, Visibility};
use rbit_localization::{Locale, LocaleResolver};
use rbit_services::{CreateRepository, Services, SignUp, issues::NewIssueInput, pulls::NewPullInput};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt;

/// Markup, both quotes, `&` before an entity, template delimiters and a bidi override.
pub const HOSTILE: &str = "<script>alert(\"x\")</script> &amp; 'q' {{ page.site_name }} \u{202E}gpj.exe\u{202C}";
/// [`HOSTILE`] as Askama writes it: escaped exactly once.
pub const ESCAPED: &str =
    "&#60;script&#62;alert(&#34;x&#34;)&#60;/script&#62; &#38;amp; &#39;q&#39; {{ page.site_name }} \u{202E}gpj.exe\u{202C}";

/// Every catalog: the nine languages people choose and the two diagnostics.
pub const LOCALES: [Locale; 11] = [
    Locale::English,
    Locale::German,
    Locale::Spanish,
    Locale::French,
    Locale::PortugueseBrazil,
    Locale::Russian,
    Locale::Japanese,
    Locale::ChineseSimplified,
    Locale::Korean,
    Locale::PseudoExpanded,
    Locale::PseudoRtl,
];

pub const REPO: &str = "/safety-owner/demo";

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub router: Router,
    pub cookie: String,
    /// The tips of `main` and `feature`, which the pull request compares.
    pub tips: (String, String),
}

pub struct Page {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

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

impl Fixture {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let sign_up =
            SignUp { username: "safety-owner".into(), email: "safety@example.com".into(), password: "correct horse battery".into() };
        let owner = services.accounts.create_user(sign_up, Some(false)).await.unwrap();
        let input = CreateRepository { name: "demo".into(), description: HOSTILE.into(), visibility: Visibility::Public };
        let created = services.repositories.create(&owner, input).await.unwrap();

        let source = directory.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-q"]);
        std::fs::write(source.join("README.md"), "# Demo\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "initial"]);
        git(&source, &["checkout", "-qb", "feature"]);
        std::fs::write(source.join("lib.rs"), "fn main() {}\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "feature"]);
        let bare = rbit_git::RepositoryStorage::new(config.storage.repositories_dir()).path_for(created.owner.id, created.repo.id);
        git(&bare, &["fetch", "-q", source.to_str().unwrap(), "main:main", "feature:feature"]);
        let tips = (git(&bare, &["rev-parse", "main"]), git(&bare, &["rev-parse", "feature"]));

        let repo = services.repositories.open(Some(&owner), "safety-owner", "demo").await.unwrap();
        let issue = NewIssueInput { title: HOSTILE.into(), body: "Body".into(), ..NewIssueInput::default() };
        services.issues.create(&owner, &repo, issue).await.unwrap();
        let pull = NewPullInput {
            base_branch: "main".into(),
            head_branch: "feature".into(),
            title: HOSTILE.into(),
            body: String::new(),
            draft: false,
        };
        services.pulls.create(&owner, &repo, &repo, pull).await.unwrap();

        let cookie =
            format!("{}={}", rbit_web::middleware::SESSION_COOKIE, services.accounts.start_session(&owner).await.unwrap().encode());
        let router = rbit_web::router(rbit_web::AppState::new(config, services))
            .layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
        Self { _directory: directory, router, cookie, tips }
    }

    pub async fn get(&self, path: &str, headers: &[(&str, &str)]) -> Page {
        let mut request = Request::builder().uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Page { status, headers, body: String::from_utf8(body.to_vec()).unwrap() }
    }
}

impl Page {
    pub fn header(&self, name: &str) -> &str {
        self.headers.get(name).map_or("", |value| value.to_str().unwrap())
    }

    /// The value of the first `name="…"` attribute in the body.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        let start = self.body.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(&self.body[start..start + self.body[start..].find('"')?])
    }
}
