//! The milestone 3 REST API end to end: releases with streamed files, collaborators who
//! then push over Git, forks — all with personal access tokens, no browser session.
//!
//! Milestone 3 workstream A (API continuation) owns this file.

use rbit_e2e::{Git, StatusCode, TestServer};
use reqwest::Method;
use serde_json::{Value, json};

/// A plain token-authenticated API client.
struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Api {
    fn new(server: &TestServer, token: &str) -> Self {
        let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().expect("API client");
        Self { http, base: server.url("/api/v1"), token: token.to_owned() }
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = if path.starts_with("http") { path.to_owned() } else { format!("{}{path}", self.base) };
        self.http.request(method, url).header("authorization", format!("Bearer {}", self.token))
    }

    async fn send(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = self.request(method, path);
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status();
        let text = response.text().await.expect("read the body");
        (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).expect("API bodies are JSON") })
    }
}

#[tokio::test]
async fn releases_collaborators_and_forks_through_the_api() {
    let server = TestServer::start_with(|config| config.releases.max_asset_bytes = 4 * 1024 * 1024);
    let git = Git::new();
    // The first account is the site administrator, who reads everything; keep that apart.
    let _root = server.sign_up("root").await;
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "private").await;
    let alices_token = alice.create_token("automation", "admin").await;
    let bobs_token = bob.create_token("automation", "write").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", &alices_token));
    let admin = Api::new(&server, &alices_token);
    let bobs = Api::new(&server, &bobs_token);
    let repo = "/repos/alice/demo";

    // A release of the pushed tag, and a file (over 1 MiB) streamed into it as the raw body.
    let (status, release) =
        admin.send(Method::POST, &format!("{repo}/releases"), Some(json!({ "tag_name": "v1.0", "body": "First" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{release}");
    let upload_url = release["upload_url"].as_str().unwrap().replace("{?name,label}", "?name=app.bin");
    let contents: Vec<u8> = (0..1_600_000u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let response = admin
        .request(Method::POST, &upload_url)
        .header("content-type", "application/x-binary")
        .body(contents.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "uploads are not held to the API's 1 MiB body limit");
    let file: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!((file["name"].as_str(), file["size"].as_u64()), (Some("app.bin"), Some(1_600_000)));

    // Downloaded through the API with the token, byte for byte; the private file is
    // invisible to Bob and to anonymous visitors.
    let file_url = file["url"].as_str().unwrap();
    let response = admin.request(Method::GET, file_url).header("accept", "application/octet-stream").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.bytes().await.unwrap().as_ref(), contents.as_slice());
    let response = bobs.request(Method::GET, file_url).header("accept", "application/octet-stream").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let browser_url = file["browser_download_url"].as_str().unwrap().replace(server.base_url(), "");
    server.client().get(&browser_url).await.expect_status(StatusCode::NOT_FOUND);
    alice.get(&browser_url).await.expect_status(StatusCode::OK);
    let (_, latest) = admin.send(Method::GET, &format!("{repo}/releases/latest"), None).await;
    assert_eq!((latest["tag_name"].as_str(), latest["assets"][0]["download_count"].as_u64()), (Some("v1.0"), Some(2)));

    // Bob cannot see the repository until he is a collaborator; then he can push.
    let bobs_url = server.git_url_as("alice", "demo", "bob", &bobs_token);
    assert_eq!(bobs.send(Method::GET, repo, None).await.0, StatusCode::NOT_FOUND);
    let (status, collaborator) = admin.send(Method::PUT, &format!("{repo}/collaborators/bob"), Some(json!({ "permission": "push" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{collaborator}");
    let (_, permission) = admin.send(Method::GET, &format!("{repo}/collaborators/bob/permission"), None).await;
    assert_eq!(permission["permission"], "write");
    let checkout = git.clone(&bobs_url);
    let pushed = git.commit_file(&checkout, "from-bob.txt", "pushed by a collaborator\n");
    git.ok(&checkout, &["push", "--quiet", &bobs_url, "HEAD:refs/heads/main"]);
    let (_, commit) = bobs.send(Method::GET, &format!("{repo}/commits/main"), None).await;
    assert_eq!(commit["sha"], pushed);

    // Bob forks it through the API; the fork stays private and lists under its parent.
    let (status, fork) = bobs.send(Method::POST, &format!("{repo}/forks"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{fork}");
    assert_eq!(
        (fork["full_name"].as_str(), fork["private"].as_bool(), fork["parent"]["full_name"].as_str()),
        (Some("bob/demo"), Some(true), Some("alice/demo"))
    );
    let (_, forks) = admin.send(Method::GET, &format!("{repo}/forks"), None).await;
    assert_eq!(forks, json!([]), "alice cannot read bob's private fork");
    let (_, forks) = bobs.send(Method::GET, &format!("{repo}/forks"), None).await;
    assert_eq!(forks[0]["full_name"], "bob/demo");
    let fork_clone = git.clone(&server.git_url_as("bob", "demo", "bob", &bobs_token));
    assert_eq!(git.ok(&fork_clone, &["rev-parse", "HEAD"]).trim(), pushed, "the fork has Bob's commit");

    // Removed again, Bob can no longer push (or even see) alice/demo.
    let (status, _) = admin.send(Method::DELETE, &format!("{repo}/collaborators/bob"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    git.commit_file(&checkout, "late.txt", "too late\n");
    git.fails(&checkout, &["push", "--quiet", &bobs_url, "HEAD:refs/heads/main"]);
    assert_eq!(bobs.send(Method::GET, repo, None).await.0, StatusCode::NOT_FOUND);
}
