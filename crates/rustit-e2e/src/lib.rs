//! # rustit-e2e
//!
//! End-to-end tests live in `tests/`; this library is the shared harness they drive the
//! application with:
//!
//! * [`TestServer`] boots the complete application exactly as `rustit serve` does
//!   (SQLite, redb, services with their background tasks, the full router) on an
//!   ephemeral `127.0.0.1` port with a temporary data directory. Each server runs on its
//!   own thread and Tokio runtime, so tests stay independent and parallel, and dropping
//!   the server shuts it down and removes its data directory.
//! * [`Client`] is a browser stand-in: a cookie jar, a same-origin `Origin` header on
//!   every request (the CSRF check) and no automatic redirects, so tests can assert on
//!   `303 See Other` responses. Helpers cover sign-up, sign-in, repositories, tokens and
//!   organisations.
//! * [`Git`] runs the real `git` CLI with an isolated home directory and configuration,
//!   no credential helpers and no prompts; credentials travel in the remote URL.
//!
//! Milestone 1 workstream T owns this crate.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub use reqwest::StatusCode;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    redirect,
};
use rustit_core::Config;
use rustit_services::Services;
use rustit_storage::{Database, KvStore};
use rustit_web::AppState;
use tempfile::TempDir;
use tokio::sync::watch;

/// The password every test account is created with (long enough for the default policy,
/// and URL-safe so it can be embedded in Git remote URLs as is).
pub const PASSWORD: &str = "correct-horse-battery-staple";

/// How long in-flight connections may take to finish once a server is asked to stop.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// The upper bound for a single HTTP request made by a [`Client`].
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

// ------------------------------------------------------------------ server

/// A running Rustit instance. Dropping it stops the server and deletes its data.
#[derive(Debug)]
pub struct TestServer {
    addr: SocketAddr,
    base_url: String,
    stop: Option<watch::Sender<bool>>,
    thread: Option<thread::JoinHandle<()>>,
    /// Dropped last, once the server thread (and every open file) is gone.
    data_dir: Option<TempDir>,
}

impl TestServer {
    /// Boots a server with the default configuration.
    pub fn start() -> Self {
        Self::start_with(|_| {})
    }

    /// Boots a server after letting `configure` adjust the configuration. The bind
    /// address, public URL and data directory are always overridden.
    pub fn start_with(configure: impl FnOnce(&mut Config)) -> Self {
        let data_dir = tempfile::Builder::new().prefix("rustit-e2e-").tempdir().expect("create a temporary data directory");
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind an ephemeral port");
        listener.set_nonblocking(true).expect("make the listener non-blocking");
        let addr = listener.local_addr().expect("read the bound address");

        let mut config = Config::default();
        configure(&mut config);
        config.server.bind = addr;
        config.server.public_url = format!("http://{addr}");
        config.storage.data_dir = data_dir.path().to_path_buf();
        let base_url = config.server.public_url.clone();

        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let thread = thread::Builder::new()
            .name(format!("rustit-e2e-{}", addr.port()))
            .spawn(move || serve(config, listener, &ready_tx, stop_rx))
            .expect("spawn the server thread");

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => panic!("the test server failed to start: {error}"),
            Err(mpsc::RecvError) => panic!("the test server thread exited during start-up"),
        }
        Self { addr, base_url, stop: Some(stop_tx), thread: Some(thread), data_dir: Some(data_dir) }
    }

    /// The bound socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://127.0.0.1:{port}`, also the configured public URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The absolute URL of `path` (which starts with `/`).
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// The anonymous clone URL of `owner/repo`.
    pub fn git_url(&self, owner: &str, repo: &str) -> String {
        format!("{}/{owner}/{repo}.git", self.base_url)
    }

    /// The clone URL of `owner/repo` with `login:secret` embedded (both must be URL-safe).
    pub fn git_url_as(&self, owner: &str, repo: &str, login: &str, secret: &str) -> String {
        format!("http://{login}:{secret}@{}/{owner}/{repo}.git", self.addr)
    }

    /// The data directory of this instance.
    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_ref().expect("the data directory lives as long as the server").path()
    }

    /// A fresh anonymous client (its own cookie jar).
    pub fn client(&self) -> Client {
        Client::new(&self.base_url)
    }

    /// Signs up `username` (email `{username}@example.test`, password [`PASSWORD`]) and
    /// returns the signed-in client. Panics unless sign-up succeeds.
    pub async fn sign_up(&self, username: &str) -> Client {
        let client = self.client();
        client.sign_up(username).await.expect_redirect("/");
        client
    }

    /// Signs `login` in with `password` and returns the signed-in client. Panics unless
    /// sign-in succeeds.
    pub async fn sign_in(&self, login: &str, password: &str) -> Client {
        let client = self.client();
        client.sign_in(login, password).await.expect_redirect("/");
        client
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(true);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(dir) = self.data_dir.take() {
            // Everything is closed by now; a failure only leaves a stray temp directory.
            let _ = dir.close();
        }
    }
}

/// The server thread: one Tokio runtime per instance, torn down completely on exit so
/// that every file in the data directory is closed before it is removed.
fn serve(config: Config, listener: std::net::TcpListener, ready: &mpsc::Sender<Result<(), String>>, stop: watch::Receiver<bool>) {
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(format!("cannot build a runtime: {error}")));
            return;
        }
    };
    runtime.block_on(async move {
        let (app, database) = match boot(config).await {
            Ok(booted) => booted,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = ready.send(Err(format!("cannot adopt the listener: {error}")));
                return;
            }
        };
        let _ = ready.send(Ok(()));

        let mut graceful = stop.clone();
        let server = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async move {
            let _ = graceful.wait_for(|stop| *stop).await;
        });
        let mut deadline = stop;
        tokio::select! {
            result = server => {
                if let Err(error) = result {
                    eprintln!("test server error: {error}");
                }
            }
            () = async move {
                let _ = deadline.wait_for(|stop| *stop).await;
                tokio::time::sleep(SHUTDOWN_GRACE).await;
            } => eprintln!("test server did not shut down gracefully in time"),
        }
        database.close().await;
    });
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
}

/// The same wiring as `rustit_server::serve::run`.
async fn boot(config: Config) -> Result<(axum::Router, Database), String> {
    let config = Arc::new(config);
    let storage = &config.storage;
    tokio::fs::create_dir_all(storage.repositories_dir()).await.map_err(|e| format!("cannot create the data directory: {e}"))?;
    let database = Database::open(&storage.sqlite_path()).await.map_err(|e| format!("cannot open SQLite: {e}"))?;
    let kv = KvStore::open(&storage.kv_path()).map_err(|e| format!("cannot open redb: {e}"))?;
    let services = Services::new(Arc::clone(&config), &database, &kv).await.map_err(|e| format!("cannot build services: {e}"))?;
    services.spawn_background();
    let app = rustit_web::router(AppState::new(Arc::clone(&config), services));
    Ok((app, database))
}

// ------------------------------------------------------------------ HTTP client

/// A browser-like HTTP client: cookies, a same-origin `Origin` header, no redirects.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    /// A client for the server at `base_url` (`http://host:port`).
    pub fn new(base_url: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(base_url).expect("base URL is a valid header value"));
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .redirect(redirect::Policy::none())
            .default_headers(headers)
            .no_proxy()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("build the HTTP client");
        Self { http, base_url: base_url.to_owned() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// `GET path`.
    pub async fn get(&self, path: &str) -> Response {
        Response::read(self.http.get(self.url(path)).send().await).await
    }

    /// `GET path` with extra request headers.
    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = self.http.get(self.url(path));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        Response::read(request.send().await).await
    }

    /// `GET path` with HTTP Basic credentials, as Git sends them.
    pub async fn get_basic(&self, path: &str, login: &str, secret: &str) -> Response {
        Response::read(self.http.get(self.url(path)).basic_auth(login, Some(secret)).send().await).await
    }

    /// `POST path` with an `application/x-www-form-urlencoded` body.
    pub async fn post(&self, path: &str, form: &[(&str, &str)]) -> Response {
        self.post_with(path, form, &[]).await
    }

    /// `POST path` with a form body and extra (or overriding) request headers.
    pub async fn post_with(&self, path: &str, form: &[(&str, &str)], headers: &[(&str, &str)]) -> Response {
        let mut request = self.http.post(self.url(path)).form(form);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        Response::read(request.send().await).await
    }

    /// Submits the sign-up form for `username` with [`PASSWORD`].
    pub async fn sign_up(&self, username: &str) -> Response {
        let email = format!("{username}@example.test");
        self.post("/signup", &[("username", username), ("email", &email), ("password", PASSWORD)]).await
    }

    /// Submits the sign-in form.
    pub async fn sign_in(&self, login: &str, password: &str) -> Response {
        self.post("/login", &[("login", login), ("password", password)]).await
    }

    /// Signs out (ends the session and clears the cookie).
    pub async fn sign_out(&self) -> Response {
        self.post("/logout", &[]).await
    }

    /// Creates `owner/name` (`visibility` is `public` or `private`); panics unless the
    /// server redirects to the new repository.
    pub async fn create_repo(&self, owner: &str, name: &str, visibility: &str) {
        self.post("/new", &[("owner", owner), ("name", name), ("description", "Created by the e2e suite"), ("visibility", visibility)])
            .await
            .expect_redirect(&format!("/{owner}/{name}"));
    }

    /// Creates a personal access token (`scope` is `read` or `write`) that never expires
    /// and returns its secret, scraped from the one page that shows it.
    pub async fn create_token(&self, name: &str, scope: &str) -> String {
        let page = self.post("/settings/tokens", &[("name", name), ("scope", scope), ("expires", "never")]).await;
        page.expect_status(StatusCode::OK);
        let secret = attribute_after(&page.body, "id=\"new-token\"", "value").expect("the created token is shown");
        assert!(secret.starts_with("rit_"), "unexpected token format: {secret}");
        secret
    }

    /// Creates an organisation owned by this client's user.
    pub async fn create_org(&self, name: &str) {
        self.post("/organizations/new", &[("name", name), ("display_name", name), ("description", "")])
            .await
            .expect_redirect(&format!("/{name}"));
    }

    /// Adds `username` to `org` with `role` (`member` or `owner`).
    pub async fn add_member(&self, org: &str, username: &str, role: &str) {
        self.post(&format!("/{org}/members"), &[("username", username), ("role", role)]).await.expect_redirect(&format!("/{org}/members"));
    }
}

/// A fully read HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl Response {
    async fn read(result: reqwest::Result<reqwest::Response>) -> Self {
        let response = result.expect("the request reaches the test server");
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.text().await.expect("read the response body");
        Self { status, headers, body }
    }

    /// The value of `name`, if present and valid UTF-8.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The `Location` header.
    pub fn location(&self) -> Option<&str> {
        self.header(header::LOCATION.as_str())
    }

    /// Whether the body contains `needle`.
    pub fn contains(&self, needle: &str) -> bool {
        self.body.contains(needle)
    }

    /// Panics (showing the start of the body) unless the status is `expected`.
    #[track_caller]
    pub fn expect_status(&self, expected: StatusCode) -> &Self {
        assert_eq!(self.status, expected, "unexpected status; body starts with:\n{}", excerpt(&self.body));
        self
    }

    /// Panics unless this is a `303 See Other` to `location`.
    #[track_caller]
    pub fn expect_redirect(&self, location: &str) -> &Self {
        self.expect_status(StatusCode::SEE_OTHER);
        assert_eq!(self.location(), Some(location), "unexpected redirect target");
        self
    }

    /// Panics unless the body contains every one of `needles`.
    #[track_caller]
    pub fn expect_contains(&self, needles: &[&str]) -> &Self {
        for needle in needles {
            assert!(self.contains(needle), "expected {needle:?} in the body; it starts with:\n{}", excerpt(&self.body));
        }
        self
    }
}

fn excerpt(body: &str) -> &str {
    let end = body.char_indices().nth(2000).map_or(body.len(), |(index, _)| index);
    &body[..end]
}

/// The value of attribute `name` on the first tag that contains `marker` (an attribute
/// written exactly as in the template, e.g. `id="new-token"`).
pub fn attribute_after(html: &str, marker: &str, name: &str) -> Option<String> {
    let start = html.find(marker)?;
    let tag_start = html[..start].rfind('<')?;
    let tag_end = start + html[start..].find('>')?;
    let tag = &html[tag_start..tag_end];
    let needle = format!(" {name}=\"");
    let value_start = tag.find(&needle)? + needle.len();
    let value_len = tag[value_start..].find('"')?;
    Some(tag[value_start..value_start + value_len].to_owned())
}

/// Every distinct number `n` appearing in `html` as `{prefix}{n}{suffix}`, in order.
pub fn numbers_between(html: &str, prefix: &str, suffix: &str) -> Vec<i64> {
    let mut found = Vec::new();
    for (index, _) in html.match_indices(prefix) {
        let rest = &html[index + prefix.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty()
            && rest[digits.len()..].starts_with(suffix)
            && let Ok(number) = digits.parse()
            && !found.contains(&number)
        {
            found.push(number);
        }
    }
    found
}

// ------------------------------------------------------------------ git CLI

/// The real `git` CLI, isolated from the machine's configuration.
///
/// `HOME` points at a temporary directory holding the only configuration Git reads
/// (system configuration is disabled), credential helpers are cleared, and prompts are
/// disabled, so a missing or rejected credential fails immediately instead of hanging.
#[derive(Debug)]
pub struct Git {
    root: TempDir,
    counter: AtomicUsize,
}

/// The outcome of one `git` invocation.
#[derive(Debug)]
pub struct GitOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Default for Git {
    fn default() -> Self {
        Self::new()
    }
}

impl Git {
    pub fn new() -> Self {
        let root = tempfile::Builder::new().prefix("rustit-e2e-git-").tempdir().expect("create a temporary git home");
        std::fs::create_dir(root.path().join("home")).expect("create the git home directory");
        std::fs::write(
            root.path().join("home").join(".gitconfig"),
            "[user]\n\tname = E2E Tester\n\temail = e2e@example.test\n\
             [init]\n\tdefaultBranch = main\n\
             [credential]\n\thelper =\n\
             [core]\n\tautocrlf = false\n\tfsmonitor = false\n\
             [advice]\n\tdetachedHead = false\n",
        )
        .expect("write the isolated git configuration");
        Self { root, counter: AtomicUsize::new(0) }
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    /// A new, not yet existing path for a working copy (`git clone` creates it).
    pub fn fresh_path(&self, label: &str) -> PathBuf {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        self.root.path().join(format!("{label}-{n}"))
    }

    /// A `git` command running in `dir` with the isolated environment.
    pub fn command(&self, dir: &Path) -> Command {
        let home = self.home();
        let mut command = Command::new("git");
        command
            .current_dir(dir)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("GCM_INTERACTIVE", "never")
            .env("LC_ALL", "C")
            .env("LANG", "C");
        for inherited in
            ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_CONFIG", "GIT_CONFIG_COUNT", "GIT_CONFIG_PARAMETERS", "SSH_ASKPASS"]
        {
            command.env_remove(inherited);
        }
        command
    }

    /// Runs `git args…` in `dir`.
    pub fn run(&self, dir: &Path, args: &[&str]) -> GitOutput {
        let Output { status, stdout, stderr } = self.command(dir).args(args).output().expect("run git (is it installed?)");
        GitOutput {
            success: status.success(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        }
    }

    /// Runs `git args…` in `dir` and returns stdout; panics with stderr on failure.
    #[track_caller]
    pub fn ok(&self, dir: &Path, args: &[&str]) -> String {
        let output = self.run(dir, args);
        assert!(output.success, "git {} failed:\n{}", args.join(" "), output.stderr);
        output.stdout
    }

    /// Runs `git args…` in `dir`, panics if it succeeds, and returns stderr.
    #[track_caller]
    pub fn fails(&self, dir: &Path, args: &[&str]) -> String {
        let output = self.run(dir, args);
        assert!(!output.success, "git {} unexpectedly succeeded:\n{}{}", args.join(" "), output.stdout, output.stderr);
        output.stderr
    }

    /// Clones `url` into a fresh directory and returns its path; panics on failure.
    #[track_caller]
    pub fn clone(&self, url: &str) -> PathBuf {
        let target = self.fresh_path("clone");
        self.ok(self.root.path(), &["clone", "--quiet", url, &target.to_string_lossy()]);
        target
    }

    /// Tries to clone `url`; returns git's stderr, panicking if the clone succeeds.
    #[track_caller]
    pub fn clone_fails(&self, url: &str) -> String {
        let target = self.fresh_path("failed-clone");
        self.fails(self.root.path(), &["clone", "--quiet", url, &target.to_string_lossy()])
    }

    /// Creates a working copy with known history (see [`Sample`]).
    pub fn sample(&self) -> Sample {
        let dir = self.fresh_path("sample");
        std::fs::create_dir_all(dir.join("src")).expect("create the sample directory");
        self.ok(&dir, &["init", "--quiet", "--initial-branch=main"]);

        write(&dir, "README.md", Sample::README);
        write(&dir, "src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n");
        self.ok(&dir, &["add", "--all"]);
        self.ok(&dir, &["commit", "--quiet", "-m", Sample::FIRST_MESSAGE]);

        write(&dir, "src/main.rs", "fn main() {\n    println!(\"hello, rustit\");\n}\n");
        write(&dir, "docs/guide.txt", Sample::GUIDE);
        self.ok(&dir, &["add", "--all"]);
        self.ok(&dir, &["commit", "--quiet", "-m", Sample::SECOND_MESSAGE]);
        self.ok(&dir, &["tag", "-a", Sample::TAG, "-m", "First release"]);

        self.ok(&dir, &["checkout", "--quiet", "-b", Sample::BRANCH]);
        write(&dir, "feature.txt", "feature work\n");
        self.ok(&dir, &["add", "--all"]);
        self.ok(&dir, &["commit", "--quiet", "-m", "Feature work"]);
        self.ok(&dir, &["checkout", "--quiet", "main"]);

        let head = self.ok(&dir, &["rev-parse", "HEAD"]).trim().to_owned();
        Sample { dir, head }
    }

    /// Commits a new file `name` in `dir` and returns the new commit id.
    pub fn commit_file(&self, dir: &Path, name: &str, contents: &str) -> String {
        write(dir, name, contents);
        self.ok(dir, &["add", "--all"]);
        self.ok(dir, &["commit", "--quiet", "-m", &format!("Add {name}")]);
        self.ok(dir, &["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// Pushes `main`, the feature branch and the tag of a [`Sample`] to `url`.
    #[track_caller]
    pub fn push_sample(&self, sample: &Sample, url: &str) {
        self.ok(&sample.dir, &["push", "--quiet", url, "main", Sample::BRANCH, Sample::TAG]);
    }
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, contents).expect("write a file into the working copy");
}

/// A working copy with a known history:
///
/// 1. `Initial commit`: `README.md`, `src/main.rs`.
/// 2. `Add the guide and greet rustit` (`main`, tagged `v1.0`): edits `src/main.rs`,
///    adds `docs/guide.txt`.
/// 3. `Feature work` on branch `feature`: adds `feature.txt`.
#[derive(Debug)]
pub struct Sample {
    pub dir: PathBuf,
    /// The commit `main` points to.
    pub head: String,
}

impl Sample {
    pub const README: &'static str = "# Sample project\n\nHello from the end-to-end suite.\n";
    pub const GUIDE: &'static str = "Plain text guide for the raw view.\n<script>alert(1)</script>\n";
    pub const FIRST_MESSAGE: &'static str = "Initial commit";
    pub const SECOND_MESSAGE: &'static str = "Add the guide and greet rustit";
    pub const TAG: &'static str = "v1.0";
    pub const BRANCH: &'static str = "feature";
}
