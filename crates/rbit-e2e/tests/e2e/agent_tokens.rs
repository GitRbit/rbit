//! Scoped tokens end to end (agent plan step 4.8): an agent's token reaches one repository
//! and does what its rights say, over the API and over Git; the owner manages tokens over
//! the API, and a limited token can make no other.
//!
//! The integration workstream (I) owns this file.

use rbit_e2e::{Git, TestServer};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

async fn send(server: &TestServer, token: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    // Redirects are answers here: a limited token must not be sent to another name.
    let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().unwrap();
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
    // Actions on, so its endpoints (annotations) answer for themselves.
    let server = TestServer::start_with(|config| config.actions.enabled = true);
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
    assert_ne!(denied, "");

    // Nothing account-wide: no list naming the other repositories, no new repository; and
    // writes its rights do not name are refused before anything is looked up.
    assert_eq!(send(&server, &agent, Method::GET, "user/repos", None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send(&server, &agent, Method::POST, "user/repos", Some(json!({ "name": "evil" }))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send(&server, &agent, Method::GET, "user", None).await.0, StatusCode::OK, "who it belongs to");
    let logs = json!({ "name": "logs", "scope": "write", "repository": "alice/rocket", "rights": ["read_logs"] });
    let (_, reader) = send(&server, &admin, Method::POST, "user/tokens", Some(logs)).await;
    let reader = reader["token"].as_str().unwrap().to_owned();
    let release = json!({ "tag_name": "v9", "target_commitish": "main" });
    let (status, body) = send(&server, &reader, Method::POST, "repos/alice/rocket/releases", Some(release)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a release makes a tag, which needs `push`: {body}");
    let resolution = "repos/alice/rocket/pulls/1/comments/1/resolution";
    let (status, body) = send(&server, &reader, Method::PUT, resolution, Some(json!({ "resolved": true }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "resolving a review thread needs `approve`: {body}");
    let (status, _) = send(&server, &agent, Method::POST, "repos/alice/rocket/forks", Some(json!({ "name": "copy" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a fork is a new repository");
    assert_eq!(send(&server, &admin, Method::GET, "repos/alice/copy", None).await.0, StatusCode::NOT_FOUND, "and none was made");
    // Each write its rights do not name is refused before anything is looked up.
    for (method, path, body) in [
        (Method::POST, "repos/alice/rocket/pulls", json!({ "title": "x", "head": "main", "base": "main" })),
        (Method::POST, "repos/alice/rocket/pulls/1/reviews", json!({ "event": "APPROVE" })),
        (Method::PUT, "repos/alice/rocket/pulls/1/reviews/1/dismissals", json!({ "message": "x" })),
        (Method::POST, "repos/alice/rocket/releases/1/assets/from-artifact", json!({ "artifact_id": 1 })),
    ] {
        assert_eq!(send(&server, &reader, method, path, Some(body)).await.0, StatusCode::FORBIDDEN, "{path} needs its right");
    }
    let artifacts = "repos/alice/rocket/agent/artifacts?sha=0000000&name=dist";
    assert_eq!(send(&server, &agent, Method::GET, artifacts, None).await.0, StatusCode::FORBIDDEN, "artifacts need `read_logs`");
    let annotations = "repos/alice/rocket/actions/jobs/1/annotations";
    assert_eq!(send(&server, &agent, Method::GET, annotations, None).await.0, StatusCode::FORBIDDEN, "annotations need `read_logs`");
    // An old name of another repository is not followed for it: that would name the new one.
    assert_eq!(send(&server, &admin, Method::PATCH, "repos/alice/vault", Some(json!({ "name": "vault-new" }))).await.0, StatusCode::OK);
    assert_eq!(send(&server, &agent, Method::GET, "repos/alice/vault", None).await.0, StatusCode::NOT_FOUND);

    // A limited token makes no tokens, even with the admin scope.
    let wide = json!({ "name": "wide", "scope": "admin", "repository": "alice/rocket" });
    let (_, wide) = send(&server, &admin, Method::POST, "user/tokens", Some(wide)).await;
    let wide = wide["token"].as_str().unwrap().to_owned();
    let (status, _) = send(&server, &wide, Method::POST, "user/tokens", Some(json!({ "name": "more", "scope": "admin" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Nor anything that needs the admin scope: the repository, its protection, its hooks.
    let changed = json!({ "description": "changed by a limited token" });
    assert_eq!(send(&server, &wide, Method::PATCH, "repos/alice/rocket", Some(changed)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send(&server, &wide, Method::GET, "repos/alice/rocket/branch_protections", None).await.0, StatusCode::FORBIDDEN);

    // Revoked, it stops at once.
    let id = row["id"].as_i64().unwrap();
    assert_eq!(send(&server, &admin, Method::DELETE, &format!("user/tokens/{id}"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&server, &agent, Method::GET, "repos/alice/rocket", None).await.0, StatusCode::UNAUTHORIZED);
}
