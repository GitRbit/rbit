//! Hostile requests to the Git LFS and archive endpoints through the real router,
//! services and repositories on disk: transfer tokens replayed on other objects and
//! repositories, object ids that alias or traverse, archive revisions that look like
//! options or paths, and a repository whose configuration names a filter command.

use std::{path::Path, process::Command, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_core::{Config, TokenScope, User, Visibility};
use rbit_services::{CreateAccessToken, CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const LFS_JSON: &str = "application/vnd.git-lfs+json";

struct Fixture {
    dir: tempfile::TempDir,
    services: Services,
    router: axum::Router,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.security.rate_limit = false;
        "https://git.example.com".clone_into(&mut config.server.public_url);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { dir, services, router }
    }

    /// A user, their `Basic` credentials (a write token) and their repositories.
    async fn user(&self, name: &str, repos: &[(&str, Visibility)]) -> (User, String) {
        let signup = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        let user = self.services.accounts.create_user(signup, Some(false)).await.unwrap();
        for (repo, visibility) in repos {
            let input = CreateRepository { name: (*repo).into(), description: String::new(), visibility: *visibility };
            self.services.repositories.create(&user, input).await.unwrap();
        }
        let input = CreateAccessToken { name: "token".into(), scope: TokenScope::Write, expires_in_days: None };
        let token = self.services.tokens.create(&user, input).await.unwrap().1;
        let basic = format!("Basic {}", STANDARD.encode(format!("{name}:{token}")));
        (user, basic)
    }

    async fn send(&self, method: &str, path: &str, authorization: Option<&str>, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
        let path = path.strip_prefix("https://git.example.com").unwrap_or(path);
        let mut request = Request::builder().method(method).uri(path);
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        if method != "GET" {
            request = request.header(header::CONTENT_TYPE, LFS_JSON).header(header::CONTENT_LENGTH, body.len());
        }
        let response = self.router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
        let status = response.status();
        (status, axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec())
    }

    async fn batch(&self, repo: &str, authorization: &str, operation: &str, content: &[u8]) -> Value {
        let body = json!({ "operation": operation, "objects": [{ "oid": oid(content), "size": content.len() }] });
        let (status, bytes) =
            self.send("POST", &format!("/{repo}/info/lfs/objects/batch"), Some(authorization), body.to_string().into()).await;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Uploads and verifies `content` in `repo` as a client would.
    async fn push_object(&self, repo: &str, authorization: &str, content: &[u8]) {
        let answer = self.batch(repo, authorization, "upload", content).await;
        let actions = &answer["objects"][0]["actions"];
        let token = actions["upload"]["header"]["Authorization"].as_str().unwrap();
        assert_eq!(self.send("PUT", actions["upload"]["href"].as_str().unwrap(), Some(token), content.to_vec()).await.0, StatusCode::OK);
        let verify = json!({ "oid": oid(content), "size": content.len() }).to_string().into_bytes();
        let token = actions["verify"]["header"]["Authorization"].as_str();
        assert_eq!(self.send("POST", actions["verify"]["href"].as_str().unwrap(), token, verify).await.0, StatusCode::OK);
    }

    /// A download action's (href, token) for `content` in `repo`.
    async fn download(&self, repo: &str, authorization: &str, content: &[u8]) -> (String, String) {
        let answer = self.batch(repo, authorization, "download", content).await;
        let action = &answer["objects"][0]["actions"]["download"];
        (action["href"].as_str().unwrap().to_owned(), action["header"]["Authorization"].as_str().unwrap().to_owned())
    }
}

fn oid(content: &[u8]) -> String {
    rbit_core::lfs::LfsOid::from_digest(Sha256::digest(content).into()).to_string()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[tokio::test]
async fn lfs_requests_outside_the_repository_scope_are_refused() {
    let fixture = Fixture::new().await;
    let (_alice, alice) = fixture.user("alice", &[("assets", Visibility::Private), ("other", Visibility::Private)]).await;
    let (_bob, bob) = fixture.user("bob", &[("mine", Visibility::Private)]).await;
    let (first, second, elsewhere) =
        (b"first private object".as_slice(), b"second private object".as_slice(), b"in another repo".as_slice());
    fixture.push_object("alice/assets", &alice, first).await;
    fixture.push_object("alice/assets", &alice, second).await;
    fixture.push_object("alice/other", &alice, elsewhere).await;
    let leaked = |bytes: &[u8]| [first, second, elsewhere].iter().any(|secret| bytes.windows(secret.len()).any(|w| w == *secret));

    // A download token names one object of one repository.
    let (href, token) = fixture.download("alice/assets", &alice, first).await;
    let (status, bytes) = fixture.send("GET", &href, Some(&token), Vec::new()).await;
    assert_eq!((status, bytes.as_slice()), (StatusCode::OK, first));
    for path in [
        format!("/alice/assets/info/lfs/objects/{}", oid(second)),
        format!("/alice/other/info/lfs/objects/{}", oid(elsewhere)),
        format!("/alice/other/info/lfs/objects/{}", oid(first)),
        format!("/bob/mine/info/lfs/objects/{}", oid(first)),
    ] {
        let (status, bytes) = fixture.send("GET", &path, Some(&token), Vec::new()).await;
        assert!(matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND), "{path}: {status}");
        assert!(!leaked(&bytes), "{path}");
    }
    // An object is linked per repository: its owner's own other repository does not have it,
    // and another account must upload the bytes before it may download them.
    let missing = format!("/alice/other/info/lfs/objects/{}", oid(first));
    assert_eq!(fixture.send("GET", &missing, Some(&alice), Vec::new()).await.0, StatusCode::NOT_FOUND);
    let answer = fixture.batch("bob/mine", &bob, "upload", first).await;
    assert!(answer["objects"][0]["actions"]["upload"].is_object(), "knowing the id is not having the object: {answer}");
    let (status, bytes) = fixture.send("GET", &format!("/bob/mine/info/lfs/objects/{}", oid(first)), Some(&bob), Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!leaked(&bytes));

    // Object ids that alias, traverse or carry extra bytes never name a stored file.
    let id = oid(first);
    for spelling in [
        id.to_uppercase(),
        format!("{id}0"),
        id[..63].to_owned(),
        format!("{id}%00"),
        format!("{id}.."),
        "..%2F..%2F..%2Frbit.db".to_owned(),
        "%2e%2e".to_owned(),
        format!("..%5C{id}"),
        format!("{}%2F{}%2F{id}", &id[..2], &id[2..4]),
    ] {
        let (status, bytes) = fixture.send("GET", &format!("/alice/assets/info/lfs/objects/{spelling}"), Some(&alice), Vec::new()).await;
        assert!(status.is_client_error(), "{spelling}: {status}");
        assert!(!leaked(&bytes), "{spelling}");
    }
}

#[tokio::test]
async fn archive_downloads_ignore_hostile_revisions_and_repository_filters() {
    let fixture = Fixture::new().await;
    let (alice, _) = fixture.user("alice", &[("code", Visibility::Public)]).await;
    let ctx = fixture.services.repositories.open(Some(&alice), "alice", "code").await.unwrap();
    let work = fixture.dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    std::fs::write(work.join(".gitattributes"), "* filter=evil\n").unwrap();
    std::fs::write(work.join("data.txt"), "data\n").unwrap();
    git(&work, &["add", "--all"]);
    git(&work, &["commit", "-q", "-m", "files"]);
    git(&work, &["push", "-q", ctx.disk_path.to_str().unwrap(), "main"]);
    let marker = fixture.dir.path().join("filter-ran");
    let command = format!("echo ran >> '{}'; cat", marker.to_string_lossy().replace('\\', "/"));
    git(&work, &["--git-dir", ctx.disk_path.to_str().unwrap(), "config", "filter.evil.smudge", &command]);

    let (status, bytes) = fixture.send("GET", "/alice/code/archive/main.tar.gz", None, Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes[..2], [0x1f, 0x8b]);
    let (status, bytes) = fixture.send("GET", "/alice/code/archive/main.zip", None, Vec::new()).await;
    assert_eq!((status, &bytes[..2]), (StatusCode::OK, b"PK".as_slice()));
    assert!(!marker.exists(), "an archive download never runs the repository's filter command");

    for revision in [
        "--output=pwned",
        "-",
        "..%2F..%2Fconfig",
        "refs%2F..%2F..%2Fconfig",
        "main:data.txt",
        "main%00",
        "%2Fetc%2Fpasswd",
        "HEAD@%7B-1%7D",
        "main%5E%7Btree%7D",
    ] {
        for suffix in [".zip", ".tar.gz"] {
            let (status, _) = fixture.send("GET", &format!("/alice/code/archive/{revision}{suffix}"), None, Vec::new()).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{revision}{suffix}");
        }
    }
    assert!(!fixture.dir.path().join("pwned").exists() && !std::env::current_dir().unwrap().join("pwned").exists());
}
