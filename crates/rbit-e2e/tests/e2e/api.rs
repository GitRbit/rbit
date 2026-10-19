//! The REST API and webhook deliveries, end to end.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! Webhook deliveries go to a local receiver on 127.0.0.1, so these servers run with
//! `[webhooks] allow_private_addresses = true`.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, http::HeaderMap, routing::post};
use hmac::{Hmac, KeyInit, Mac};
use rbit_e2e::{Git, PASSWORD, StatusCode, TestServer, numbers_between};
use sha2::Sha256;

// ------------------------------------------------------------------ a webhook receiver

/// One request the receiver accepted.
#[derive(Debug, Clone)]
struct Received {
    headers: HeaderMap,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("deliveries are JSON")
    }

    /// Checks `X-Hub-Signature-256` against `secret` with an independent HMAC.
    fn signed_with(&self, secret: &str) -> bool {
        let Some(hex) = self.header("x-hub-signature-256").and_then(|value| value.strip_prefix("sha256=")) else { return false };
        let Some(signature) = decode_hex(hex) else { return false };
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).expect("any key length");
        mac.update(self.body.as_bytes());
        mac.verify_slice(&signature).is_ok() && self.header("x-rbit-signature-256") == self.header("x-hub-signature-256")
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok())).collect()
}

type Inbox = Arc<Mutex<Vec<Received>>>;

/// An HTTP endpoint on 127.0.0.1 that records every `POST /hook` and answers 204.
#[derive(Debug, Clone)]
struct Receiver {
    url: String,
    inbox: Inbox,
}

impl Receiver {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the receiver");
        let url = format!("http://{}/hook", listener.local_addr().expect("receiver address"));
        let inbox = Inbox::default();
        let app = axum::Router::new().route("/hook", post(receive)).with_state(Arc::clone(&inbox));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve the receiver") });
        Self { url, inbox }
    }

    /// Waits until `count` deliveries arrived and returns them in arrival order.
    async fn wait_for(&self, count: usize) -> Vec<Received> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let received = self.inbox.lock().expect("inbox").clone();
            if received.len() >= count {
                return received;
            }
            assert!(Instant::now() < deadline, "only {} of {count} deliveries arrived", received.len());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn count(&self) -> usize {
        self.inbox.lock().expect("inbox").len()
    }
}

async fn receive(State(inbox): State<Inbox>, headers: HeaderMap, body: String) -> StatusCode {
    inbox.lock().expect("inbox").push(Received { headers, body });
    StatusCode::NO_CONTENT
}

fn server_for_webhooks() -> TestServer {
    TestServer::start_with(|config| config.webhooks.allow_private_addresses = true)
}

// ------------------------------------------------------------------ webhook settings UI

#[tokio::test]
async fn repository_webhooks_are_managed_in_settings_and_deliver_signed_events() {
    let server = server_for_webhooks();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "private").await;
    let receiver = Receiver::start().await;
    let hooks = "/alice/demo/settings/hooks";

    alice.get("/alice/demo/settings").await.expect_status(StatusCode::OK).expect_contains(&[hooks, "Webhooks"]);
    alice.get(hooks).await.expect_status(StatusCode::OK).expect_contains(&["No webhooks", "Add webhook"]);
    alice.get(&format!("{hooks}/new")).await.expect_status(StatusCode::OK).expect_contains(&["Payload URL", "name=\"event_push\""]);
    for outsider in [&bob, &server.client()] {
        outsider.get(hooks).await.expect_status(StatusCode::NOT_FOUND);
    }

    // Validation re-renders the form with what was entered (but never the secret).
    let rejected = alice.post(hooks, &[("url", &receiver.url), ("secret", "topsecret"), ("active", "on")]).await;
    rejected.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Choose at least one event.", &receiver.url]);
    assert!(!rejected.contains("topsecret"));
    let internal = TestServer::start();
    let strict = internal.sign_up("carol").await;
    strict.create_repo("carol", "demo", "public").await;
    strict
        .post("/carol/demo/settings/hooks", &[("url", "http://127.0.0.1:9/hook"), ("event_push", "on"), ("active", "on")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["internal address"]);
    drop(internal);

    let created = alice.post(hooks, &[("url", &receiver.url), ("secret", "topsecret"), ("event_push", "on"), ("active", "on")]).await;
    created.expect_status(StatusCode::SEE_OTHER);
    let hook_url = created.location().expect("redirect to the hook").to_owned();
    assert!(hook_url.starts_with(&format!("{hooks}/")), "{hook_url}");

    // The ping arrives, signed.
    let ping = &receiver.wait_for(1).await[0];
    assert_eq!(ping.header("x-rbit-event"), Some("ping"));
    assert_eq!(ping.header("x-github-event"), Some("ping"));
    assert!(ping.signed_with("topsecret"), "the HMAC verifies");
    assert!(!ping.signed_with("wrong secret"));
    assert_eq!(ping.json()["repository"]["full_name"], "alice/demo");

    // The hook page shows the secret as configured and the delivery once recorded.
    let page = wait_for_page(&alice, &hook_url, |body| body.contains("Delivered · 204")).await;
    page.expect_contains(&["Recent deliveries", ">ping<", "Configured — leave blank to keep it", "Send ping"]);
    assert!(!page.contains("topsecret"), "the secret is write-only");
    alice.get(hooks).await.expect_contains(&["127.0.0.1", "Pushes", "Delivered · 204"]);

    // A push is delivered as one signed `push` event.
    let sample = git.sample();
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    let push = &receiver.wait_for(2).await[1];
    assert_eq!(push.header("x-rbit-event"), Some("push"));
    assert!(push.signed_with("topsecret"));
    let payload = push.json();
    assert_eq!(payload["ref"], "refs/heads/main");
    assert_eq!(payload["after"], sample.head);
    assert_eq!(payload["total_commits"], 2);
    assert_eq!(payload["pusher"]["name"], "alice");

    // The delivery page shows what was sent and received; redelivery sends it again.
    let page = wait_for_page(&alice, &hook_url, |body| body.contains(">push<") && body.matches("Delivered · 204").count() == 2).await;
    let ids = numbers_between(&page.body, &format!("{hook_url}/deliveries/"), "\"");
    let push_page = alice.get(&format!("{hook_url}/deliveries/{}", ids[0])).await;
    push_page.expect_status(StatusCode::OK).expect_contains(&["X-Rbit-Signature-256", "refs/heads/main", "Redeliver", "204"]);
    assert!(!push_page.contains("topsecret"));
    let again = alice.post(&format!("{hook_url}/deliveries/{}/redeliver", ids[0]), &[]).await;
    again.expect_status(StatusCode::SEE_OTHER);
    assert_eq!(receiver.wait_for(3).await[2].body, push.body, "the same payload");
    alice.post(&format!("{hook_url}/ping"), &[]).await.expect_redirect(&format!("{hook_url}?pinged=1"));
    receiver.wait_for(4).await;
    bob.post(&format!("{hook_url}/ping"), &[]).await.expect_status(StatusCode::NOT_FOUND);

    // Editing keeps the secret unless it is removed; deactivated hooks stop delivering.
    let edited = alice.post(&hook_url, &[("url", &receiver.url), ("event_push", "on"), ("event_release", "on")]).await;
    edited.expect_redirect(&format!("{hook_url}?saved=1"));
    alice.get(&format!("{hook_url}?saved=1")).await.expect_contains(&["Webhook saved.", "Configured — leave blank to keep it"]);
    let before = receiver.count();
    git.commit_file(&sample.dir, "later.txt", "inactive hooks hear nothing\n");
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(receiver.count(), before, "inactive hooks receive nothing");

    // Deleting needs the confirmation box.
    let delete = format!("{hook_url}/delete");
    alice.post(&delete, &[]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Tick the box to confirm."]);
    alice.post(&delete, &[("confirm", "on")]).await.expect_redirect(hooks);
    alice.get(hooks).await.expect_contains(&["No webhooks"]);
    alice.get(&hook_url).await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn organisation_webhooks_fire_for_new_repositories() {
    let server = server_for_webhooks();
    let olivia = server.sign_up("olivia").await;
    let mason = server.sign_up("mason").await;
    olivia.create_org("acme").await;
    olivia.add_member("acme", "mason", "member").await;
    let receiver = Receiver::start().await;
    let hooks = "/acme/settings/hooks";

    olivia.get("/acme/settings").await.expect_status(StatusCode::OK).expect_contains(&[hooks]);
    olivia.get(hooks).await.expect_status(StatusCode::OK).expect_contains(&["No webhooks", "or any of its repositories"]);
    mason.get(hooks).await.expect_status(StatusCode::NOT_FOUND);

    olivia.post(hooks, &[("url", &receiver.url), ("event_repository", "on"), ("active", "on")]).await.expect_status(StatusCode::SEE_OTHER);
    assert_eq!(receiver.wait_for(1).await[0].json()["organization"]["login"], "acme");
    mason.create_repo("acme", "rocket", "private").await;
    let created = receiver.wait_for(2).await[1].json();
    assert_eq!(created["action"], "created");
    assert_eq!(created["repository"]["full_name"], "acme/rocket");
    assert_eq!(created["sender"]["login"], "mason");
    assert!(receiver.wait_for(2).await[1].header("x-hub-signature-256").is_none(), "no secret, no signature");
}

/// Polls `path` until its body satisfies `ready` (deliveries are recorded asynchronously).
async fn wait_for_page(client: &rbit_e2e::Client, path: &str, ready: impl Fn(&str) -> bool) -> rbit_e2e::Response {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let page = client.get(path).await;
        page.expect_status(StatusCode::OK);
        if ready(&page.body) || Instant::now() > deadline {
            return page;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ------------------------------------------------------------------ REST API

/// A JSON response.
#[derive(Debug)]
struct ApiResponse {
    status: StatusCode,
    headers: reqwest::header::HeaderMap,
    json: serde_json::Value,
}

impl ApiResponse {
    #[track_caller]
    fn expect(&self, status: StatusCode) -> &serde_json::Value {
        assert_eq!(self.status, status, "unexpected status; body: {}", self.json);
        &self.json
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

/// A plain API client: no cookies, an optional bearer token.
#[derive(Debug, Clone)]
struct Api {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl Api {
    fn new(server: &TestServer, token: Option<&str>) -> Self {
        let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("API client");
        Self { http, base: server.url("/api/v1"), token: token.map(str::to_owned) }
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<serde_json::Value>) -> ApiResponse {
        let url = if path.starts_with("http") { path.to_owned() } else { format!("{}{path}", self.base) };
        let mut request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status();
        let headers = response.headers().clone();
        let text = response.text().await.expect("read the body");
        let json = if text.is_empty() { serde_json::Value::Null } else { serde_json::from_str(&text).expect("API bodies are JSON") };
        ApiResponse { status, headers, json }
    }

    async fn get(&self, path: &str) -> ApiResponse {
        self.send(reqwest::Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> ApiResponse {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }
}

#[tokio::test]
async fn tokens_read_the_api_and_page_through_repositories() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    for name in ["one", "two", "three"] {
        alice.create_repo("alice", name, "public").await;
    }
    alice.create_repo("alice", "vault", "private").await;
    let token = alice.create_token("api", "read").await;
    let api = Api::new(&server, Some(&token));

    let me = api.get("/user").await;
    let me = me.expect(StatusCode::OK);
    assert_eq!((me["login"].as_str(), me["type"].as_str()), (Some("alice"), Some("User")));
    assert_eq!(me["html_url"], server.url("/alice"));
    Api::new(&server, None).get("/user").await.expect(StatusCode::UNAUTHORIZED);
    // The same token written with the legacy `rit_` prefix of earlier releases.
    let legacy = Api::new(&server, Some(&token.replacen("rbit_", "rit_", 1))).get("/user").await;
    assert_eq!(legacy.expect(StatusCode::OK)["login"], "alice");
    let forged = Api::new(&server, Some(&format!("rbit_{}", "A".repeat(43)))).get("/user").await;
    forged.expect(StatusCode::UNAUTHORIZED);
    assert!(forged.header("www-authenticate").is_some_and(|value| value.starts_with("Bearer")));

    // Four repositories, two per page: follow `Link` from the first page to the last.
    let mut url = "/user/repos?per_page=2".to_owned();
    let mut names = Vec::new();
    let mut pages = 0;
    loop {
        let page = api.get(&url).await;
        page.expect(StatusCode::OK);
        assert_eq!(page.header("x-total-count"), Some("4"));
        names.extend(page.json.as_array().unwrap().iter().map(|repo| repo["name"].as_str().unwrap().to_owned()));
        pages += 1;
        let link = page.header("link").unwrap_or_default().to_owned();
        let next = link.split(", ").find(|part| part.ends_with("rel=\"next\"")).map(|part| part[1..part.find('>').unwrap()].to_owned());
        let Some(next) = next else {
            assert!(link.contains("rel=\"prev\"") && link.contains("rel=\"first\""), "{link}");
            break;
        };
        url = next;
    }
    names.sort();
    assert_eq!((pages, names), (2, vec!["one".to_owned(), "three".to_owned(), "two".to_owned(), "vault".to_owned()]));

    // Anonymous callers see public repositories only; other users' private ones are 404.
    let public = Api::new(&server, None).get("/users/alice/repos").await;
    assert_eq!(public.header("x-total-count"), Some("3"));
    let bobs = bob.create_token("bob", "read").await;
    let as_bob = Api::new(&server, Some(&bobs));
    as_bob.get("/repos/alice/vault").await.expect(StatusCode::NOT_FOUND);
    as_bob.get("/repos/alice/vault/commits").await.expect(StatusCode::NOT_FOUND);
    assert_eq!(api.get("/repos/alice/vault").await.expect(StatusCode::OK)["private"], true);

    // The OpenAPI document is served; unknown paths are JSON 404s.
    assert!(api.get("/openapi.json").await.expect(StatusCode::OK)["paths"]["/user"].is_object());
    assert_eq!(api.get("/no/such/thing").await.expect(StatusCode::NOT_FOUND)["message"], "Not Found");
}

#[tokio::test]
async fn repositories_branches_commits_and_stars_through_the_api() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let write = Api::new(&server, Some(&alice.create_token("write", "write").await));
    let admin = Api::new(&server, Some(&alice.create_token("admin", "admin").await));

    let created = write.post("/user/repos", serde_json::json!({ "name": "demo", "description": "API made", "private": true })).await;
    let created = created.expect(StatusCode::CREATED).clone();
    assert_eq!((created["full_name"].as_str(), created["private"].as_bool()), (Some("alice/demo"), Some(true)));
    assert_eq!(created["clone_url"], server.git_url("alice", "demo"));
    write.post("/user/repos", serde_json::json!({ "name": "demo" })).await.expect(StatusCode::CONFLICT);
    assert_eq!(write.get("/repos/alice/demo/commits").await.expect(StatusCode::CONFLICT)["message"], "Git Repository is empty.");

    let sample = git.sample();
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "topic/api"]);
    git.ok(&sample.dir, &["checkout", "--quiet", "main"]);
    git.ok(
        &sample.dir,
        &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main", "feature", "topic/api", "v1.0"],
    );

    let branches = write.get("/repos/alice/demo/branches").await;
    let names: Vec<_> =
        branches.expect(StatusCode::OK).as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, ["feature", "main", "topic/api"]);
    let slashed = write.get("/repos/alice/demo/branches/topic/api").await;
    assert_eq!(slashed.expect(StatusCode::OK)["commit"]["sha"], sample.head);
    write.get("/repos/alice/demo/branches/nope").await.expect(StatusCode::NOT_FOUND);
    let tags = write.get("/repos/alice/demo/tags").await;
    assert_eq!(tags.expect(StatusCode::OK)[0]["name"], "v1.0");
    assert_eq!(tags.json[0]["commit"]["sha"], sample.head, "annotated tags are peeled");

    let first = write.get("/repos/alice/demo/commits?per_page=1").await;
    assert_eq!(first.expect(StatusCode::OK)[0]["sha"], sample.head);
    let link = first.header("link").unwrap_or_default();
    assert!(link.contains("page=2>; rel=\"next\"") && !link.contains("rel=\"last\""), "{link}");
    let second = write.get("/repos/alice/demo/commits?per_page=1&page=2").await;
    assert_eq!(second.expect(StatusCode::OK)[0]["commit"]["message"], "Initial commit");
    assert!(!second.header("link").unwrap_or_default().contains("rel=\"next\""), "the last page has no next");
    let feature = write.get("/repos/alice/demo/commits?sha=feature").await;
    assert_eq!(feature.expect(StatusCode::OK)[0]["commit"]["message"], "Feature work");
    let single = write.get("/repos/alice/demo/commits/v1.0").await;
    assert_eq!(single.expect(StatusCode::OK)["sha"], sample.head);
    write.get("/repos/alice/demo/commits/0000000000000000000000000000000000000000").await.expect(StatusCode::NOT_FOUND);

    // Stars.
    let star = write.send(reqwest::Method::PUT, "/user/starred/alice/demo", None).await;
    star.expect(StatusCode::NO_CONTENT);
    assert_eq!(write.get("/user/starred").await.expect(StatusCode::OK)[0]["full_name"], "alice/demo");
    write.send(reqwest::Method::DELETE, "/user/starred/alice/demo", None).await.expect(StatusCode::NO_CONTENT);
    assert_eq!(write.get("/user/starred").await.header("x-total-count"), Some("0"));

    // Settings and deletion need the admin scope.
    let settings = serde_json::json!({ "name": "renamed", "description": null, "private": false, "default_branch": "feature" });
    write.send(reqwest::Method::PATCH, "/repos/alice/demo", Some(settings.clone())).await.expect(StatusCode::FORBIDDEN);
    let response = admin.send(reqwest::Method::PATCH, "/repos/alice/demo", Some(settings)).await;
    let renamed = response.expect(StatusCode::OK);
    assert_eq!((renamed["full_name"].as_str(), renamed["private"].as_bool()), (Some("alice/renamed"), Some(false)));
    assert_eq!((renamed["default_branch"].as_str(), renamed["description"].is_null()), (Some("feature"), true));
    let bad = admin.send(reqwest::Method::PATCH, "/repos/alice/renamed", Some(serde_json::json!({ "default_branch": "missing" }))).await;
    bad.expect(StatusCode::UNPROCESSABLE_ENTITY);
    write.send(reqwest::Method::DELETE, "/repos/alice/renamed", None).await.expect(StatusCode::FORBIDDEN);
    admin.send(reqwest::Method::DELETE, "/repos/alice/renamed", None).await.expect(StatusCode::NO_CONTENT);
    admin.get("/repos/alice/renamed").await.expect(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn forks_name_their_parent_through_the_api() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "lib", "public").await;
    git.push_sample(&git.sample(), &server.git_url_as("alice", "lib", "alice", PASSWORD));
    bob.post("/alice/lib/fork", &[("owner", "bob"), ("name", "")]).await.expect_redirect("/bob/lib");
    let api = Api::new(&server, None);

    let fork = api.get("/repos/bob/lib").await;
    let fork = fork.expect(StatusCode::OK);
    assert_eq!(fork["fork"], true);
    assert_eq!((fork["parent"]["full_name"].as_str(), fork["parent"]["fork"].as_bool()), (Some("alice/lib"), Some(false)));
    let parent = api.get("/repos/alice/lib").await;
    let parent = parent.expect(StatusCode::OK);
    assert_eq!(parent["fork"], false);
    assert!(parent.get("parent").is_none(), "only forks have a parent");
    assert_eq!(api.get("/users/bob/repos").await.expect(StatusCode::OK)[0]["fork"], true, "lists carry the flag too");
}

#[tokio::test]
async fn a_hook_created_through_the_api_receives_a_signed_push() {
    let server = server_for_webhooks();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "public").await;
    let receiver = Receiver::start().await;
    let admin = Api::new(&server, Some(&alice.create_token("hooks", "admin").await));
    let write = Api::new(&server, Some(&alice.create_token("push", "write").await));
    let hooks = "/repos/alice/demo/hooks";

    // Only admins with the admin scope manage hooks; others who can see the repository get 403.
    let body = serde_json::json!({ "config": { "url": receiver.url, "content_type": "json", "secret": "api-secret" }, "events": ["push"] });
    write.post(hooks, body.clone()).await.expect(StatusCode::FORBIDDEN);
    Api::new(&server, Some(&bob.create_token("bob", "admin").await)).get(hooks).await.expect(StatusCode::FORBIDDEN);
    let invalid = admin.post(hooks, serde_json::json!({ "config": { "url": "ftp://example.com" } })).await;
    assert_eq!(invalid.expect(StatusCode::UNPROCESSABLE_ENTITY)["errors"][0]["field"], "config.url");
    let unknown = admin.post(hooks, serde_json::json!({ "config": { "url": receiver.url }, "events": ["discussion"] })).await;
    assert_eq!(unknown.expect(StatusCode::UNPROCESSABLE_ENTITY)["errors"][0]["field"], "events");

    let created = admin.post(hooks, body).await;
    let hook = created.expect(StatusCode::CREATED).clone();
    let hook_url = created.header("location").expect("Location").to_owned();
    assert_eq!(hook["config"]["secret"], "********", "the secret is never returned");
    assert_eq!(hook["events"], serde_json::json!(["push"]));
    assert!(!created.json.to_string().contains("api-secret"));
    assert_eq!(receiver.wait_for(1).await[0].header("x-rbit-event"), Some("ping"));

    // Push, then check the delivery: headers, the independently verified HMAC, the payload.
    let sample = git.sample();
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    let push = &receiver.wait_for(2).await[1];
    assert_eq!(push.header("x-rbit-event"), Some("push"));
    assert_eq!(push.header("x-github-event"), Some("push"));
    assert!(push.header("x-rbit-delivery").is_some_and(|guid| guid.len() == 36));
    assert!(push.header("user-agent").is_some_and(|agent| agent.starts_with("rbit-Hookshot/")));
    assert!(push.signed_with("api-secret"), "X-Hub-Signature-256 verifies with the secret");
    let payload = push.json();
    assert_eq!((payload["ref"].as_str(), payload["after"].as_str()), (Some("refs/heads/main"), Some(sample.head.as_str())));
    assert_eq!(payload["repository"]["full_name"], "alice/demo");
    assert_eq!(payload["commits"].as_array().unwrap().len(), 2);

    // The delivery log through the API, and redelivery.
    let deliveries = wait_for_deliveries(&admin, &format!("{hook_url}/deliveries"), 2).await;
    assert_eq!(deliveries[0]["event"], "push");
    assert_eq!(deliveries[1]["event"], "ping");
    let detail = admin.get(deliveries[0]["url"].as_str().unwrap()).await;
    let detail = detail.expect(StatusCode::OK);
    assert_eq!(detail["request"]["payload"]["ref"], "refs/heads/main");
    assert!(detail["response"]["headers"].is_object());
    assert_eq!(detail["request"]["headers"]["X-Rbit-Event"], "push");
    let again = admin.post(&format!("{}/attempts", deliveries[0]["url"].as_str().unwrap()), serde_json::json!({})).await;
    let again = again.expect(StatusCode::ACCEPTED);
    assert_ne!(again["id"], deliveries[0]["id"], "redelivery gets its own log entry");
    assert_ne!(again["guid"], deliveries[0]["guid"], "redelivery gets its own delivery identifier");
    // The worker is notified before the API reads the new delivery, so it may have
    // claimed or completed the attempt by the time the response is serialized.
    assert!(matches!(again["status"].as_str(), Some("pending" | "delivering" | "succeeded")), "{again}");
    assert_eq!(receiver.wait_for(3).await[2].body, push.body);

    // Changing and deleting.
    let patched =
        admin.send(reqwest::Method::PATCH, &hook_url, Some(serde_json::json!({ "active": false, "config": { "secret": "" } }))).await;
    let patched = patched.expect(StatusCode::OK);
    assert_eq!((patched["active"].as_bool(), patched["config"]["secret"].is_null()), (Some(false), true));
    admin.send(reqwest::Method::POST, &format!("{hook_url}/pings"), None).await.expect(StatusCode::NO_CONTENT);
    assert!(receiver.wait_for(4).await[3].header("x-hub-signature-256").is_none(), "the secret was removed");
    assert_eq!(admin.get(hooks).await.header("x-total-count"), Some("1"));
    admin.send(reqwest::Method::DELETE, &hook_url, None).await.expect(StatusCode::NO_CONTENT);
    admin.get(&hook_url).await.expect(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn organisation_hooks_through_the_api() {
    let server = server_for_webhooks();
    let olivia = server.sign_up("olivia").await;
    let mason = server.sign_up("mason").await;
    olivia.create_org("acme").await;
    olivia.add_member("acme", "mason", "member").await;
    let receiver = Receiver::start().await;
    let owner = Api::new(&server, Some(&olivia.create_token("owner", "admin").await));
    let member = Api::new(&server, Some(&mason.create_token("member", "admin").await));

    let body = serde_json::json!({ "config": { "url": receiver.url }, "events": ["repository", "push"] });
    member.post("/orgs/acme/hooks", body.clone()).await.expect(StatusCode::FORBIDDEN);
    owner.post("/orgs/nope/hooks", body.clone()).await.expect(StatusCode::NOT_FOUND);
    let hook = owner.post("/orgs/acme/hooks", body).await;
    assert_eq!(hook.expect(StatusCode::CREATED)["type"], "Organization");
    receiver.wait_for(1).await;

    // A member creates a repository in the organisation through the API; the hook hears it.
    let created = member.post("/orgs/acme/repos", serde_json::json!({ "name": "tools" })).await;
    assert_eq!(created.expect(StatusCode::CREATED)["owner"]["login"], "acme");
    let event = receiver.wait_for(2).await[1].json();
    assert_eq!((event["action"].as_str(), event["repository"]["full_name"].as_str()), (Some("created"), Some("acme/tools")));
    assert_eq!(owner.get("/orgs/acme/members").await.header("x-total-count"), Some("2"));
    assert_eq!(owner.get("/orgs/acme").await.expect(StatusCode::OK)["type"], "Organization");
}

/// Polls a delivery list until it holds `count` deliveries that are no longer pending.
async fn wait_for_deliveries(api: &Api, url: &str, count: usize) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let page = api.get(url).await;
        let deliveries = page.expect(StatusCode::OK).as_array().unwrap().clone();
        let done = deliveries.iter().filter(|delivery| delivery["status"] == "succeeded").count();
        if done >= count {
            return deliveries;
        }
        assert!(Instant::now() < deadline, "deliveries did not finish: {deliveries:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
