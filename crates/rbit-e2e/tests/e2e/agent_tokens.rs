//! Scoped tokens end to end (agent plan step 4.8): an agent's token reaches one repository
//! and does what its rights say, over the API and over Git; the owner manages tokens over
//! the API, and a limited token can make no other.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, TestServer};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

async fn send(server: &TestServer, token: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut request = http.request(method, format!("{}/api/v1/{path}", server.base_url())).bearer_auth(token);
    if let Some(body) = body {
        request = request.header("Content-Type", "application/json").body(body.to_string());
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    (status, if text.is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap() })
}

#[tokio::test]
async fn a_limited_token_reaches_one_repository_with_its_rights_only() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "rocket", "public").await;
    alice.create_repo("alice", "vault", "private").await;
    let admin = alice.create_token("owner", "admin").await;

    // The owner makes an agent token over the API: rocket only, push only.
    let new = json!({ "name": "agent", "scope": "write", "repository": "alice/rocket", "rights": ["push"] });
    let (status, made) = send(&server, &admin, Method::POST, "user/tokens", Some(new)).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["rights"], json!(["push"]), "{made}");
    let agent = made["token"].as_str().unwrap().to_owned();
    let (_, listed) = send(&server, &admin, Method::GET, "user/tokens", None).await;
    let row = listed["tokens"].as_array().unwrap().iter().find(|token| token["name"] == "agent").cloned().unwrap();
    assert!(row.get("token").is_none_or(Value::is_null), "secrets are never listed: {row}");

    // The API: rocket only; statuses need a right it lacks.
    assert_eq!(send(&server, &agent, Method::GET, "repos/alice/rocket", None).await.0, StatusCode::OK);
    assert_eq!(
        send(&server, &agent, Method::GET, "repos/alice/vault", None).await.0,
        StatusCode::NOT_FOUND,
        "the other repository is not there"
    );
    let git = Git::new();
    let sample = git.sample();
    let url = |repo: &str, token: &str| server.git_url_as("alice", repo, "alice", token);
    let head = git.commit_file(&sample.dir, "fuel.txt", "full\n");
    git.ok(&sample.dir, &["push", "--quiet", &url("rocket", &agent), "main"]);
    let status = json!({ "state": "success", "context": "ci" });
    let (refused, body) = send(&server, &agent, Method::POST, &format!("repos/alice/rocket/statuses/{head}"), Some(status)).await;
    assert_eq!(refused, StatusCode::FORBIDDEN, "{body}");
    assert!(body["message"].as_str().unwrap().contains("statuses"), "{body}");

    // Git: pushing to the other repository is refused.
    let denied = git.fails(&sample.dir, &["push", &url("vault", &agent), "main"]);
    assert!(!denied.is_empty());

    // A limited token makes no tokens, even with the admin scope.
    let wide = json!({ "name": "wide", "scope": "admin", "repository": "alice/rocket" });
    let (_, wide) = send(&server, &admin, Method::POST, "user/tokens", Some(wide)).await;
    let wide = wide["token"].as_str().unwrap().to_owned();
    let (status, _) = send(&server, &wide, Method::POST, "user/tokens", Some(json!({ "name": "more", "scope": "admin" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Revoked, it stops at once.
    let id = row["id"].as_i64().unwrap();
    assert_eq!(send(&server, &admin, Method::DELETE, &format!("user/tokens/{id}"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&server, &agent, Method::GET, "repos/alice/rocket", None).await.0, StatusCode::UNAUTHORIZED);
}
