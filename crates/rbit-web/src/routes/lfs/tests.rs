//! The LFS API in-process: batch, basic transfers, locks, authentication, errors, and
//! LFS objects on the blob and raw pages.

use std::{path::Path, process::Command, sync::Arc};

use axum::{
    Router,
    body::{Body, Bytes},
    http::{Request, Response, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rbit_core::{Config, Permission, TokenScope, User, Visibility, lfs::LfsPointer};
use rbit_services::{CreateAccessToken, CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use super::LFS_JSON;
use crate::AppState;

const BASE: &str = "https://git.example.com";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    router: Router,
}

impl Fixture {
    async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        BASE.clone_into(&mut config.server.public_url);
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let router = crate::router(AppState::new(config, services.clone()));
        Self { _dir: dir, database, services, router }
    }

    async fn user(&self, name: &str) -> User {
        let signup = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        self.services.accounts.create_user(signup, Some(false)).await.unwrap()
    }

    /// `Basic` credentials with a new personal access token of `scope` as the password.
    async fn basic(&self, user: &User, scope: TokenScope) -> String {
        let input = CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None };
        let token = self.services.tokens.create(user, input).await.unwrap().1;
        format!("Basic {}", STANDARD.encode(format!("{}:{token}", user.name())))
    }

    async fn repo(&self, owner: &User, name: &str, visibility: Visibility) {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility };
        self.services.repositories.create(owner, input).await.unwrap();
    }

    /// Any request; the response and its body.
    async fn send(&self, method: &str, path: &str, authorization: Option<&str>, body: Vec<u8>) -> (Response<Body>, Bytes) {
        let mut request = Request::builder().method(method).uri(path.strip_prefix(BASE).unwrap_or(path));
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        if method != "GET" {
            request = request.header(header::CONTENT_TYPE, LFS_JSON).header(header::CONTENT_LENGTH, body.len());
        }
        let response = self.router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        (Response::from_parts(parts, Body::empty()), bytes)
    }

    /// A JSON request; status and JSON body.
    async fn json(&self, method: &str, path: &str, authorization: Option<&str>, body: &Value) -> (StatusCode, Value) {
        let (response, bytes) = self.send(method, path, authorization, body.to_string().into_bytes()).await;
        if !bytes.is_empty() {
            assert_eq!(response.headers()[header::CONTENT_TYPE], LFS_JSON, "{path}");
        }
        (response.status(), serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn batch(&self, repo: &str, authorization: Option<&str>, operation: &str, objects: &[&[u8]]) -> (StatusCode, Value) {
        let objects: Vec<Value> = objects.iter().map(|content| json!({ "oid": oid(content), "size": content.len() })).collect();
        let body = json!({ "operation": operation, "transfers": ["basic"], "objects": objects, "hash_algo": "sha256" });
        self.json("POST", &format!("/{repo}/info/lfs/objects/batch"), authorization, &body).await
    }

    /// Uploads `content` to `repo` through the batch API, as a client would.
    async fn push_object(&self, repo: &str, authorization: &str, content: &[u8]) {
        let (status, answer) = self.batch(repo, Some(authorization), "upload", &[content]).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let upload = &answer["objects"][0]["actions"]["upload"];
        let header = upload["header"]["Authorization"].as_str().unwrap();
        let (response, _) = self.send("PUT", upload["href"].as_str().unwrap(), Some(header), content.to_vec()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let verify = &answer["objects"][0]["actions"]["verify"];
        let body = json!({ "oid": oid(content), "size": content.len() });
        let (status, _) = self.json("POST", verify["href"].as_str().unwrap(), verify["header"]["Authorization"].as_str(), &body).await;
        assert_eq!(status, StatusCode::OK);
    }

    /// Commits `files` (path, contents) to `main` of `owner/name` directly on disk.
    async fn commit(&self, owner: &User, name: &str, files: &[(&str, &[u8])]) {
        let ctx = self.services.repositories.open(Some(owner), owner.name().as_str(), name).await.unwrap();
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
        for (path, contents) in files {
            std::fs::write(work.path().join(path), contents).unwrap();
        }
        git(work.path(), &["add", "--all"]);
        git(work.path(), &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "files"]);
        git(work.path(), &["push", "--quiet", ctx.disk_path.to_str().unwrap(), "main"]);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn oid(content: &[u8]) -> String {
    rbit_core::lfs::LfsOid::from_digest(Sha256::digest(content).into()).to_string()
}

fn pointer(content: &[u8]) -> Vec<u8> {
    LfsPointer { oid: rbit_core::lfs::LfsOid::parse(&oid(content)).unwrap(), size: content.len() as u64 }.to_text().into_bytes()
}

#[tokio::test]
async fn upload_verify_and_download_through_the_batch_api() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    fixture.repo(&alice, "assets", Visibility::Private).await;
    let auth = fixture.basic(&alice, TokenScope::Write).await;
    let content = b"\x00\x01binary design file".as_slice();

    let (status, answer) = fixture.batch("alice/assets.git", Some(&auth), "upload", &[content]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["transfer"], "basic");
    let object = &answer["objects"][0];
    assert_eq!(object["oid"], oid(content));
    let upload = &object["actions"]["upload"];
    assert_eq!(upload["href"], format!("{BASE}/alice/assets.git/info/lfs/objects/{}", oid(content)));
    assert!(upload["header"]["Authorization"].as_str().unwrap().starts_with("Bearer lfs1."));
    assert_eq!(upload["expires_in"], 3600);
    assert_eq!(object["actions"]["verify"]["href"], format!("{BASE}/alice/assets.git/info/lfs/objects/{}/verify", oid(content)));

    fixture.push_object("alice/assets", &auth, content).await;
    let (_, again) = fixture.batch("alice/assets", Some(&auth), "upload", &[content]).await;
    assert_eq!(again["objects"][0].get("actions"), None, "already stored: nothing to upload");

    // Download through the endpoint without `.git`, as clients derive it from such remotes.
    let (status, answer) = fixture.batch("alice/assets", Some(&auth), "download", &[content]).await;
    assert_eq!(status, StatusCode::OK);
    let download = &answer["objects"][0]["actions"]["download"];
    let (response, bytes) =
        fixture.send("GET", download["href"].as_str().unwrap(), download["header"]["Authorization"].as_str(), Vec::new()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(response.headers()[header::CONTENT_LENGTH], content.len().to_string().as_str());
    assert_eq!(&bytes[..], content);
    // Basic credentials work on the transfer endpoints too.
    let (response, bytes) = fixture.send("GET", download["href"].as_str().unwrap(), Some(&auth), Vec::new()).await;
    assert_eq!((response.status(), &bytes[..]), (StatusCode::OK, content));
}

#[tokio::test]
async fn access_follows_the_repository() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    fixture.repo(&alice, "public", Visibility::Public).await;
    fixture.repo(&alice, "private", Visibility::Private).await;
    let alice_auth = fixture.basic(&alice, TokenScope::Write).await;
    let bob_auth = fixture.basic(&bob, TokenScope::Write).await;
    fixture.push_object("alice/public", &alice_auth, b"public object").await;

    // Anonymous: public downloads work, uploads and private repositories ask for credentials.
    let (status, answer) = fixture.batch("alice/public", None, "download", &[b"public object"]).await;
    assert_eq!(status, StatusCode::OK);
    let download = &answer["objects"][0]["actions"]["download"];
    let (response, bytes) =
        fixture.send("GET", download["href"].as_str().unwrap(), download["header"]["Authorization"].as_str(), Vec::new()).await;
    assert_eq!((response.status(), &bytes[..]), (StatusCode::OK, b"public object".as_slice()));
    let (response, _) =
        fixture.send("POST", "/alice/public/info/lfs/objects/batch", None, br#"{"operation":"upload","objects":[]}"#.to_vec()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers()["lfs-authenticate"].to_str().unwrap().starts_with("Basic"));
    assert_eq!(fixture.batch("alice/private", None, "download", &[]).await.0, StatusCode::UNAUTHORIZED);

    // Signed in without access: the private repository does not exist; the public one is read-only.
    assert_eq!(fixture.batch("alice/private", Some(&bob_auth), "download", &[]).await.0, StatusCode::NOT_FOUND);
    let (status, answer) = fixture.batch("alice/public", Some(&bob_auth), "upload", &[b"x"]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");

    // A read-scoped token caps access; a collaborator may write.
    let read_only = fixture.basic(&alice, TokenScope::Read).await;
    assert_eq!(fixture.batch("alice/private", Some(&read_only), "upload", &[b"x"]).await.0, StatusCode::FORBIDDEN);
    let ctx = fixture.services.repositories.open(Some(&alice), "alice", "private").await.unwrap();
    fixture.services.collaborators.add(&alice, &ctx, "bob", Permission::Write).await.unwrap();
    assert_eq!(fixture.batch("alice/private", Some(&bob_auth), "upload", &[b"x"]).await.0, StatusCode::OK);

    // Wrong credentials are challenged; forged tokens too.
    let wrong = format!("Basic {}", STANDARD.encode("alice:rbit_wrongwrongwrongwrongwrongwrongwrongwrongwro"));
    assert_eq!(fixture.batch("alice/public", Some(&wrong), "download", &[]).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.batch("alice/public", Some("Bearer lfs1.AAAA.BBBB"), "download", &[]).await.0, StatusCode::UNAUTHORIZED);

    // Pull mirrors refuse uploads, with the reason.
    fixture.database.mirrors().create(ctx.repo.repo.id, "https://example.com/upstream.git", 60).await.unwrap();
    let (status, answer) = fixture.batch("alice/private", Some(&alice_auth), "upload", &[b"x"]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(answer["message"].as_str().unwrap().contains("mirror"), "{answer}");
}

#[tokio::test]
async fn errors_are_reported_per_request_and_per_object() {
    let fixture = Fixture::new(|config| config.lfs.max_object_bytes = 10).await;
    let alice = fixture.user("alice").await;
    fixture.repo(&alice, "assets", Visibility::Public).await;
    let auth = fixture.basic(&alice, TokenScope::Write).await;
    let batch = "/alice/assets/info/lfs/objects/batch";

    let (response, _) = fixture.send("POST", batch, Some(&auth), b"{not json".to_vec()).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        fixture.json("POST", batch, Some(&auth), &json!({ "operation": "delete", "objects": [] })).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let tus = json!({ "operation": "download", "transfers": ["tus"], "objects": [] });
    assert_eq!(fixture.json("POST", batch, Some(&auth), &tus).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let md5 = json!({ "operation": "download", "objects": [], "hash_algo": "md5" });
    assert_eq!(fixture.json("POST", batch, Some(&auth), &md5).await.0, StatusCode::CONFLICT);

    let objects = json!([
        { "oid": oid(b"missing"), "size": 7 },
        { "oid": "../../etc/passwd", "size": 1 },
        { "oid": oid(b"far too large"), "size": 13 },
    ]);
    let (_, download) = fixture.json("POST", batch, Some(&auth), &json!({ "operation": "download", "objects": objects })).await;
    assert_eq!(download["objects"][0]["error"]["code"], 404);
    assert_eq!(download["objects"][1]["error"]["code"], 422);
    let (_, upload) = fixture.json("POST", batch, Some(&auth), &json!({ "operation": "upload", "objects": objects })).await;
    assert_eq!(upload["objects"][2]["error"]["code"], 413);
    assert!(upload["objects"][0]["actions"]["upload"].is_object());

    // The transfer token covers one object and its size.
    let action = &upload["objects"][0]["actions"]["upload"];
    let token = action["header"]["Authorization"].as_str().unwrap();
    let other = format!("/alice/assets/info/lfs/objects/{}", oid(b"other"));
    assert_eq!(fixture.send("PUT", &other, Some(token), b"other".to_vec()).await.0.status(), StatusCode::FORBIDDEN);
    let (response, body) = fixture.send("PUT", action["href"].as_str().unwrap(), Some(token), b"MISSING".to_vec()).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(String::from_utf8_lossy(&body).contains("does not match"));
    let big = format!("/alice/assets/info/lfs/objects/{}", oid(b"eleven byte"));
    assert_eq!(fixture.send("PUT", &big, Some(&auth), b"eleven byte".to_vec()).await.0.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        fixture.send("GET", "/alice/assets/info/lfs/objects/nothex", Some(&auth), Vec::new()).await.0.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let missing = format!("/alice/assets/info/lfs/objects/{}", oid(b"missing"));
    assert_eq!(fixture.send("GET", &missing, Some(&auth), Vec::new()).await.0.status(), StatusCode::NOT_FOUND);
    let verify = json!({ "oid": oid(b"missing"), "size": 7 });
    assert_eq!(fixture.json("POST", &format!("{missing}/verify"), Some(&auth), &verify).await.0, StatusCode::NOT_FOUND);
    assert_eq!(fixture.batch("alice/nope", Some(&auth), "download", &[]).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn disabled_lfs_is_not_found() {
    let fixture = Fixture::new(|config| config.lfs.enabled = false).await;
    let alice = fixture.user("alice").await;
    fixture.repo(&alice, "assets", Visibility::Public).await;
    let auth = fixture.basic(&alice, TokenScope::Write).await;
    assert_eq!(fixture.batch("alice/assets", Some(&auth), "download", &[]).await.0, StatusCode::NOT_FOUND);
    assert_eq!(fixture.json("GET", "/alice/assets/info/lfs/locks", Some(&auth), &Value::Null).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn locks_can_be_created_listed_verified_and_released() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    fixture.repo(&alice, "assets", Visibility::Public).await;
    let ctx = fixture.services.repositories.open(Some(&alice), "alice", "assets").await.unwrap();
    fixture.services.collaborators.add(&alice, &ctx, "bob", Permission::Write).await.unwrap();
    let alice_auth = fixture.basic(&alice, TokenScope::Write).await;
    let bob_auth = fixture.basic(&bob, TokenScope::Write).await;
    let locks = "/alice/assets.git/info/lfs/locks";

    let (status, created) =
        fixture.json("POST", locks, Some(&bob_auth), &json!({ "path": "logo.psd", "ref": { "name": "refs/heads/main" } })).await;
    assert_eq!(status, StatusCode::CREATED);
    let lock = &created["lock"];
    assert_eq!((lock["path"].as_str(), lock["owner"]["name"].as_str()), (Some("logo.psd"), Some("bob")));
    assert!(lock["locked_at"].as_str().unwrap().ends_with('Z'));
    let id = lock["id"].as_str().unwrap().to_owned();
    let (status, conflict) = fixture.json("POST", locks, Some(&alice_auth), &json!({ "path": "logo.psd" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["lock"]["id"], id.as_str());
    fixture.json("POST", locks, Some(&alice_auth), &json!({ "path": "song.wav" })).await;

    // Anyone who can read lists; anonymous callers cannot lock.
    let (status, listed) = fixture.json("GET", &format!("{locks}?path=logo.psd"), None, &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["locks"].as_array().unwrap().len(), 1);
    let (_, page) = fixture.json("GET", &format!("{locks}?limit=1"), None, &Value::Null).await;
    assert_eq!(page["next_cursor"], id.as_str());
    assert_eq!(fixture.json("POST", locks, None, &json!({ "path": "x" })).await.0, StatusCode::UNAUTHORIZED);

    let (status, verified) = fixture.json("POST", &format!("{locks}/verify"), Some(&bob_auth), &json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(verified["ours"][0]["path"], "logo.psd");
    assert_eq!(verified["theirs"][0]["path"], "song.wav");

    // Only the owner unlocks, unless an administrator forces it.
    let unlock = format!("{locks}/{id}/unlock");
    let alice_unlock = fixture.json("POST", &unlock, Some(&alice_auth), &json!({})).await;
    assert_eq!(alice_unlock.0, StatusCode::FORBIDDEN);
    let capped = fixture.json("POST", &unlock, Some(&alice_auth), &json!({ "force": true })).await;
    assert_eq!(capped.0, StatusCode::FORBIDDEN, "a write-scoped token cannot administer");
    let alice_admin = fixture.basic(&alice, TokenScope::Admin).await;
    let (status, released) = fixture.json("POST", &unlock, Some(&alice_admin), &json!({ "force": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(released["lock"]["id"], id.as_str());
    assert_eq!(fixture.json("POST", &unlock, Some(&bob_auth), &json!({})).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_blob_page_shows_lfs_objects_and_raw_serves_them() {
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    fixture.repo(&alice, "assets", Visibility::Public).await;
    let auth = fixture.basic(&alice, TokenScope::Write).await;
    let text = b"fn main() { println!(\"stored in LFS\"); }\n".as_slice();
    let image = b"\x89PNG\r\n\x1a\n not really a png".as_slice();
    fixture.push_object("alice/assets", &auth, text).await;
    fixture.push_object("alice/assets", &auth, image).await;
    let (missing_pointer, image_pointer, text_pointer) = (pointer(b"never uploaded"), pointer(image), pointer(text));
    fixture.commit(&alice, "assets", &[("main.rs", &text_pointer), ("logo.png", &image_pointer), ("missing.bin", &missing_pointer)]).await;

    let (response, page) = fixture.send("GET", "/alice/assets/blob/main/main.rs", None, Vec::new()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let page = String::from_utf8_lossy(&page);
    assert!(page.contains("Stored with Git LFS"));
    assert!(page.contains("stored in LFS") && !page.contains("git-lfs.github.com/spec"), "the object, not the pointer");
    assert!(page.contains(&format!("{} Bytes", text.len())) || page.contains(&text.len().to_string()), "the object's size");

    let (_, page) = fixture.send("GET", "/alice/assets/blob/main/logo.png", None, Vec::new()).await;
    assert!(String::from_utf8_lossy(&page).contains(r#"<img src="/alice/assets/raw/main/logo.png""#));

    let (_, page) = fixture.send("GET", "/alice/assets/blob/main/missing.bin", None, Vec::new()).await;
    let page = String::from_utf8_lossy(&page);
    assert!(page.contains("Object not on this server") && page.contains("git-lfs.github.com/spec"), "the pointer is shown");

    let (response, bytes) = fixture.send("GET", "/alice/assets/raw/main/main.rs", None, Vec::new()).await;
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    assert_eq!(&bytes[..], text);
    let (response, bytes) = fixture.send("GET", "/alice/assets/raw/main/logo.png", None, Vec::new()).await;
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "sandbox; default-src 'none'");
    assert_eq!(&bytes[..], image);
    let (_, bytes) = fixture.send("GET", "/alice/assets/raw/main/missing.bin", None, Vec::new()).await;
    assert_eq!(&bytes[..], &missing_pointer[..], "without the object, raw serves the pointer");
}
