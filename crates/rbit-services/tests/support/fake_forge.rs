//! An in-process fake of GitHub's REST API (also answering Gitea's `/api/v1`), a Git
//! remote served over Git's "dumb" HTTP protocol from a local bare repository, a Git LFS
//! batch endpoint and release file downloads — everything an import talks to, on an
//! ephemeral port.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file. The services
//! tests include it as a module, and so does the end-to-end scenario (`#[path]`).
//!
//! The source repository `acme/rocket` has:
//!
//! | # | kind | state | notes |
//! |---|------|-------|-------|
//! | 1 | pull | merged | head `refs/pull/1/head`, merge commit on `main`, one comment |
//! | 2 | pull | open | same-repository head `feature` |
//! | 3 | issue | closed, not planned | locked, label `bug`, milestone `v1`, two comments |
//! | 4 | pull | open | from the fork `fork/rocket` (head only as `refs/pull/4/head`) |
//! | 5 | pull | open | its head ref is missing, so it must be imported closed |
//! | 6 | issue | open | assigned to `octocat`, label `enhancement`, milestone `v2`, one comment |
//! | 8 | issue | closed | (7 was deleted on the source) |
//!
//! plus three labels, two milestones (one closed), a published release `v1.0` with one
//! file (served through a redirect, like GitHub's) and a draft, and one LFS object.
//! Listings are paged two items at a time with `Link` headers; the first request for
//! comments is refused by a used-up rate limit that resets a second later.

#![allow(dead_code, reason = "each including test uses a different part")]

use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use axum::{
    Router,
    body::Bytes,
    extract::{Path as UrlPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Listings are paged this small so that pagination is exercised.
const PAGE_SIZE: usize = 2;
pub const RELEASE_FILE: &[u8] = b"rocket binary, version 1.0\n";
pub const LFS_CONTENT: &[u8] = b"a large file kept in Git LFS\n";

/// A running fake forge.
pub struct FakeForge {
    /// `http://127.0.0.1:<port>`.
    pub base: String,
    pub token: String,
    state: Arc<ForgeState>,
    _dir: tempfile::TempDir,
}

struct ForgeState {
    base: String,
    token: String,
    remote_dir: PathBuf,
    data: HashMap<&'static str, Vec<Value>>,
    repo: Value,
    users: HashMap<String, Value>,
    lfs: (String, Vec<u8>),
    rate_limited_once: AtomicBool,
    /// `(method path, Authorization header)` of every request.
    requests: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeForge {
    /// Builds the source repository and starts serving it. `clone_url` overrides the
    /// clone URL the API reports (to make the Git copy fail). An empty `token` serves the
    /// Git remote without authentication.
    pub async fn start(token: &str, clone_url: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let remote_dir = build_repository(dir.path());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let lfs_oid = hex(&Sha256::digest(LFS_CONTENT));
        let state = Arc::new(ForgeState {
            repo: json!({
                "name": "rocket",
                "full_name": "acme/rocket",
                "description": "To the moon",
                "private": false,
                "default_branch": "main",
                "clone_url": clone_url.map_or_else(|| format!("{base}/acme/rocket.git"), str::to_owned),
                "html_url": format!("{base}/acme/rocket"),
            }),
            data: fixture_data(&base, &remote_dir),
            users: HashMap::from([
                ("octocat".to_owned(), json!({"login": "octocat", "html_url": format!("{base}/octocat"), "email": "octo@example.com"})),
                ("hubot".to_owned(), json!({"login": "hubot", "html_url": format!("{base}/hubot"), "email": null})),
            ]),
            lfs: (lfs_oid, LFS_CONTENT.to_vec()),
            rate_limited_once: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
            base: base.clone(),
            token: token.to_owned(),
            remote_dir,
        });
        let app = Router::new()
            .route("/api/{version}/repos/{owner}/{repo}", get(repository))
            .route("/api/{version}/repos/{owner}/{repo}/{*rest}", get(listing))
            .route("/api/{version}/users/{login}", get(user))
            .route("/acme/rocket.git/info/lfs/objects/batch", post(lfs_batch))
            .route("/acme/rocket.git/{*path}", get(git_file))
            .route("/lfs/{oid}", get(lfs_object))
            .route("/downloads/{name}", get(download))
            .with_state(Arc::clone(&state));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { base, token: token.to_owned(), state, _dir: dir }
    }

    /// The requests received so far: `"GET /path"` and the `Authorization` header.
    pub fn requests(&self) -> Vec<(String, Option<String>)> {
        self.state.requests.lock().unwrap().clone()
    }

    pub fn lfs_object(&self) -> (String, Vec<u8>) {
        self.state.lfs.clone()
    }

    /// The source repository on disk.
    pub fn bare(&self) -> &Path {
        &self.state.remote_dir
    }
}

// ------------------------------------------------------------------ the source repository

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Octo Cat", "-c", "user.email=octo@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stderr(Stdio::inherit())
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, content: &str) -> String {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", &format!("Add {file}")]);
    git(dir, &["rev-parse", "HEAD"])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// The source's history (see the module docs), as a bare repository ready for dumb HTTP.
fn build_repository(root: &Path) -> PathBuf {
    let work = root.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q"]);
    commit(&work, "README.md", "# Rocket\n");
    let oid = hex(&Sha256::digest(LFS_CONTENT));
    let pointer = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {}\n", LFS_CONTENT.len());
    commit(&work, "model.bin", &pointer);
    git(&work, &["tag", "v1.0"]);

    git(&work, &["checkout", "-q", "-b", "merged-work"]);
    let merged_head = commit(&work, "engine.rs", "fn main() {}\n");
    git(&work, &["checkout", "-q", "main"]);
    git(&work, &["merge", "-q", "--no-ff", "-m", "Merge pull request #1", "merged-work"]);
    git(&work, &["checkout", "-q", "-b", "feature"]);
    let feature = commit(&work, "feature.txt", "feature\n");
    git(&work, &["checkout", "-q", "main"]);
    git(&work, &["checkout", "-q", "-b", "patch"]);
    let fork_head = commit(&work, "patch.txt", "from a fork\n");
    git(&work, &["checkout", "-q", "main"]);

    let bare = root.join("rocket.git");
    git(root, &["clone", "-q", "--bare", "work", "rocket.git"]);
    git(&bare, &["branch", "-D", "patch", "merged-work"]);
    git(&bare, &["update-ref", "refs/pull/1/head", &merged_head]);
    git(&bare, &["update-ref", "refs/pull/2/head", &feature]);
    git(&bare, &["update-ref", "refs/pull/4/head", &fork_head]);
    git(&bare, &["update-ref", "refs/pull/4/merge", &fork_head]);
    git(&bare, &["update-server-info"]);
    bare
}

fn user_json(base: &str, login: &str) -> Value {
    json!({"login": login, "html_url": format!("{base}/{login}")})
}

/// The API's listings, in the order the source returns them.
fn fixture_data(base: &str, remote_dir: &Path) -> HashMap<&'static str, Vec<Value>> {
    let sha = |name: &str| git(remote_dir, &["rev-parse", name]);
    let main = sha("main");
    let label = |name: &str, color: &str| json!({"name": name, "color": color, "description": format!("{name} things")});
    let labels = vec![label("bug", "d73a4a"), label("enhancement", "#A2EEEF"), label("good first issue", "7057ff")];
    let milestones = vec![
        json!({"title": "v1", "description": "First", "state": "closed", "due_on": "2024-03-01T08:00:00Z",
               "created_at": "2024-01-01T00:00:00Z", "closed_at": "2024-03-02T00:00:00Z"}),
        json!({"title": "v2", "description": "", "state": "open", "due_on": null, "created_at": "2024-02-01T00:00:00Z"}),
    ];
    let issue = |number: u32, title: &str, state: &str, pull: bool, extra: Value| {
        let mut value = json!({
            "number": number,
            "title": title,
            "body": format!("Body of #{number}"),
            "user": user_json(base, "octocat"),
            "state": state,
            "labels": [],
            "assignees": [],
            "milestone": null,
            "locked": false,
            "created_at": format!("2024-01-{:02}T10:00:00Z", number),
            "updated_at": format!("2024-02-{:02}T10:00:00Z", number),
            "closed_at": if state == "closed" { json!(format!("2024-02-{:02}T09:00:00Z", number)) } else { json!(null) },
            "html_url": format!("{base}/acme/rocket/issues/{number}"),
        });
        if pull {
            value["pull_request"] = json!({"url": format!("{base}/api/v3/repos/acme/rocket/pulls/{number}")});
        }
        if let (Some(target), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            target.extend(extra.clone());
        }
        value
    };
    let issues = vec![
        issue(1, "Add the engine", "closed", true, json!({})),
        issue(2, "Add a feature", "open", true, json!({"user": user_json(base, "hubot")})),
        issue(
            3,
            "It crashes",
            "closed",
            false,
            json!({"state_reason": "not_planned", "locked": true,
            "labels": [{"name": "bug"}], "milestone": {"title": "v1"}}),
        ),
        issue(4, "Patch from a fork", "open", true, json!({})),
        issue(5, "Lost work", "open", true, json!({})),
        issue(
            6,
            "Make it faster",
            "open",
            false,
            json!({"labels": [{"name": "enhancement"}], "milestone": {"title": "v2"},
            "assignees": [user_json(base, "octocat")]}),
        ),
        issue(8, "Old report", "closed", false, json!({"state_reason": "completed", "user": null})),
    ];
    let branch = |name: &str, sha: &str, repo: &str| json!({"ref": name, "sha": sha, "repo": {"full_name": repo}});
    let pulls = vec![
        json!({"number": 1, "head": branch("merged-work", &sha("refs/pull/1/head"), "acme/rocket"), "base": branch("main", &main, "acme/rocket"),
               "merged_at": "2024-02-01T09:00:00Z", "merge_commit_sha": main, "draft": false, "merged_by": user_json(base, "octocat")}),
        json!({"number": 2, "head": branch("feature", &sha("feature"), "acme/rocket"), "base": branch("main", &main, "acme/rocket"),
               "merged_at": null, "merge_commit_sha": null, "draft": true}),
        json!({"number": 4, "head": branch("patch", &sha("refs/pull/4/head"), "fork/rocket"), "base": branch("main", &main, "acme/rocket"),
               "merged_at": null, "merge_commit_sha": null}),
        json!({"number": 5, "head": branch("lost", &"f".repeat(40), "gone/rocket"), "base": branch("main", &main, "acme/rocket"),
               "merged_at": null, "merge_commit_sha": null}),
    ];
    let comment = |number: u32, login: &str, body: &str, at: &str| {
        json!({"user": user_json(base, login), "body": body, "created_at": at, "updated_at": at,
               "issue_url": format!("{base}/api/v3/repos/acme/rocket/issues/{number}")})
    };
    let comments = vec![
        comment(1, "hubot", "Looks good", "2024-01-20T10:00:00Z"),
        comment(3, "hubot", "Same here", "2024-01-21T10:00:00Z"),
        comment(3, "octocat", "Won't fix", "2024-01-22T10:00:00Z"),
        comment(6, "octocat", "Working on it", "2024-01-23T10:00:00Z"),
    ];
    let releases = vec![
        json!({"tag_name": "v2.0", "name": "Two (draft)", "body": "Soon", "draft": true, "prerelease": false,
               "created_at": "2024-03-01T00:00:00Z", "published_at": null, "author": user_json(base, "octocat"), "assets": []}),
        json!({"tag_name": "v1.0", "name": "One", "body": "First release", "draft": false, "prerelease": false,
               "created_at": "2024-02-10T00:00:00Z", "published_at": "2024-02-11T00:00:00Z", "author": user_json(base, "hubot"),
               "assets": [{"name": "rocket.tar.gz", "size": RELEASE_FILE.len(), "content_type": "application/gzip",
                           "url": format!("{base}/api/v3/repos/acme/rocket/releases/assets/1"),
                           "browser_download_url": format!("{base}/downloads/rocket.tar.gz")}]}),
    ];
    HashMap::from([
        ("labels", labels),
        ("milestones", milestones),
        ("issues", issues),
        ("pulls", pulls),
        ("issues/comments", comments),
        ("releases", releases),
    ])
}

// ------------------------------------------------------------------ handlers

type Shared = State<Arc<ForgeState>>;

fn record(state: &ForgeState, what: String, headers: &HeaderMap) {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
    state.requests.lock().unwrap().push((what, auth));
}

/// `None` when the request may go on; a 401 for a wrong token.
fn check_api_token(state: &ForgeState, headers: &HeaderMap) -> Option<Response> {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())?;
    let valid = auth == format!("Bearer {}", state.token) || auth == format!("token {}", state.token);
    (!valid).then(|| (StatusCode::UNAUTHORIZED, axum::Json(json!({"message": "Bad credentials"}))).into_response())
}

async fn repository(State(state): Shared, UrlPath((_, owner, repo)): UrlPath<(String, String, String)>, headers: HeaderMap) -> Response {
    record(&state, format!("GET /repos/{owner}/{repo}"), &headers);
    if let Some(refused) = check_api_token(&state, &headers) {
        return refused;
    }
    if (owner.as_str(), repo.as_str()) != ("acme", "rocket") {
        return (StatusCode::NOT_FOUND, axum::Json(json!({"message": "Not Found"}))).into_response();
    }
    axum::Json(state.repo.clone()).into_response()
}

async fn user(State(state): Shared, UrlPath((_, login)): UrlPath<(String, String)>, headers: HeaderMap) -> Response {
    record(&state, format!("GET /users/{login}"), &headers);
    match state.users.get(&login) {
        Some(user) => axum::Json(user.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn listing(
    State(state): Shared,
    UrlPath((version, _, _, rest)): UrlPath<(String, String, String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    record(&state, format!("GET /{rest}"), &headers);
    if let Some(refused) = check_api_token(&state, &headers) {
        return refused;
    }
    if rest == "releases/assets/1" {
        // Like GitHub: the API answers with a redirect to the file's storage.
        return (StatusCode::FOUND, [(header::LOCATION, format!("{}/downloads/rocket.tar.gz", state.base))]).into_response();
    }
    if rest == "issues/comments" && !state.rate_limited_once.swap(true, Ordering::SeqCst) {
        let resets_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 1;
        return (
            StatusCode::FORBIDDEN,
            [("x-ratelimit-remaining", "0".to_owned()), ("x-ratelimit-reset", resets_at.to_string())],
            "rate limit exceeded",
        )
            .into_response();
    }
    let Some(items) = state.data.get(rest.as_str()) else { return StatusCode::NOT_FOUND.into_response() };
    let mut items = items.clone();
    if query.get("direction").map(String::as_str) == Some("desc") {
        items.reverse();
    }
    let page: usize = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1).max(1);
    let start = (page - 1) * PAGE_SIZE;
    let slice: Vec<Value> = items.iter().skip(start).take(PAGE_SIZE).cloned().collect();
    let mut response = axum::Json(slice).into_response();
    if start + PAGE_SIZE < items.len() {
        let others = query.iter().filter(|(k, _)| *k != "page").fold(String::new(), |mut others, (k, v)| {
            let _ = write!(others, "&{k}={v}");
            others
        });
        let next = format!("<{}/api/{version}/repos/acme/rocket/{rest}?page={}{others}>; rel=\"next\"", state.base, page + 1);
        response.headers_mut().insert(header::LINK, next.parse().unwrap());
    }
    response
}

/// Whether `headers` carry the token as Git and LFS send it (Basic, token as password).
/// Whether a Git or LFS request carries the token. With an empty token the remote is
/// public, as a pull mirror's upstream must be (mirrors send no credentials).
fn has_git_token(state: &ForgeState, headers: &HeaderMap) -> bool {
    use base64::Engine as _;
    if state.token.is_empty() {
        return true;
    }
    let expected = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{}", state.token)));
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(expected.as_str())
}

async fn git_file(State(state): Shared, UrlPath(path): UrlPath<String>, headers: HeaderMap) -> Response {
    record(&state, format!("GET /acme/rocket.git/{path}"), &headers);
    if !has_git_token(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"git\"")]).into_response();
    }
    if path.split('/').any(|segment| segment == ".." || segment.is_empty()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(state.remote_dir.join(&path)).await {
        Ok(bytes) => bytes.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn lfs_batch(State(state): Shared, headers: HeaderMap, body: Bytes) -> Response {
    record(&state, "POST /acme/rocket.git/info/lfs/objects/batch".to_owned(), &headers);
    if !has_git_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let request: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(request["operation"], "download");
    let objects: Vec<Value> = request["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| {
            let oid = object["oid"].as_str().unwrap();
            if oid == state.lfs.0 {
                json!({"oid": oid, "size": state.lfs.1.len(), "actions": {"download": {
                    "href": format!("{}/lfs/{oid}", state.base), "header": {"Authorization": "RemoteAuth lfs-download"}}}})
            } else {
                json!({"oid": oid, "size": object["size"], "error": {"code": 404, "message": "Object does not exist"}})
            }
        })
        .collect();
    ([(header::CONTENT_TYPE, "application/vnd.git-lfs+json")], axum::Json(json!({"transfer": "basic", "objects": objects}))).into_response()
}

async fn lfs_object(State(state): Shared, UrlPath(oid): UrlPath<String>, headers: HeaderMap) -> Response {
    record(&state, format!("GET /lfs/{oid}"), &headers);
    let authorised = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some("RemoteAuth lfs-download");
    if !authorised || oid != state.lfs.0 {
        return StatusCode::NOT_FOUND.into_response();
    }
    state.lfs.1.clone().into_response()
}

async fn download(State(state): Shared, UrlPath(name): UrlPath<String>, headers: HeaderMap) -> Response {
    record(&state, format!("GET /downloads/{name}"), &headers);
    if name != "rocket.tar.gz" {
        return StatusCode::NOT_FOUND.into_response();
    }
    ([(header::CONTENT_TYPE, "application/gzip")], RELEASE_FILE).into_response()
}
