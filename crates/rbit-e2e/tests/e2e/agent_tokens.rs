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

    // A limited token is never an administrator: asking for one with the admin scope is
    // refused, and one stored before acts with write (it makes no tokens, nothing below).
    let admin_limited = json!({ "name": "wide", "scope": "admin", "repository": "alice/rocket" });
    let (status, body) = send(&server, &admin, Method::POST, "user/tokens", Some(admin_limited)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let wide = json!({ "name": "wide", "scope": "write", "repository": "alice/rocket" });
    let (_, wide) = send(&server, &admin, Method::POST, "user/tokens", Some(wide)).await;
    let wide = wide["token"].as_str().unwrap().to_owned();
    let (status, _) = send(&server, &wide, Method::POST, "user/tokens", Some(json!({ "name": "more", "scope": "admin" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Nor anything that needs the admin scope: the repository, its protection, its hooks.
    let changed = json!({ "description": "changed by a limited token" });
    assert_eq!(send(&server, &wide, Method::PATCH, "repos/alice/rocket", Some(changed)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send(&server, &wide, Method::GET, "repos/alice/rocket/branch_protections", None).await.0, StatusCode::FORBIDDEN);

    // A token limited to a repository that is deleted reaches no repository, not every one.
    let doomed = json!({ "name": "doomed", "scope": "write", "repository": "alice/vault-new" });
    let (_, doomed) = send(&server, &admin, Method::POST, "user/tokens", Some(doomed)).await;
    let doomed = doomed["token"].as_str().unwrap().to_owned();
    assert_eq!(send(&server, &doomed, Method::GET, "repos/alice/vault-new", None).await.0, StatusCode::OK);
    assert_eq!(send(&server, &admin, Method::DELETE, "repos/alice/vault-new", None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&server, &doomed, Method::GET, "repos/alice/rocket", None).await.0, StatusCode::NOT_FOUND, "fails closed");
    assert_eq!(send(&server, &doomed, Method::GET, "user/repos", None).await.0, StatusCode::FORBIDDEN, "still limited");

    // Revoked, it stops at once.
    let id = row["id"].as_i64().unwrap();
    assert_eq!(send(&server, &admin, Method::DELETE, &format!("user/tokens/{id}"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&server, &agent, Method::GET, "repos/alice/rocket", None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_limited_token_reaches_no_other_repository_through_a_pull_requests_head() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "rocket", "public").await;
    let admin = alice.create_token("owner", "admin").await;
    let git = Git::new();
    let sample = git.sample();
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "rocket", "alice", rbit_e2e::PASSWORD), "main"]);
    // bob forks rocket and pushes a branch alice herself may read.
    let bobs = bob.create_token("bob", "write").await;
    let (status, fork) = send(&server, &bobs, Method::POST, "repos/alice/rocket/forks", Some(json!({}))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{fork}");
    git.commit_file(&sample.dir, "loot.txt", "bob's work\n");
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("bob", "rocket", "bob", rbit_e2e::PASSWORD), "HEAD:loot"]);
    let limited = json!({ "name": "agent", "scope": "write", "repository": "alice/rocket", "rights": ["push"] });
    let (_, made) = send(&server, &admin, Method::POST, "user/tokens", Some(limited)).await;
    let agent = made["token"].as_str().unwrap().to_owned();

    // A token limited to rocket sees bob's fork as a head that does not exist, everywhere.
    let missing = json!({ "title": "x", "head": "bob:nothing-here", "base": "main" });
    let (absent, _) = send(&server, &agent, Method::POST, "repos/alice/rocket/pulls", Some(missing)).await;
    let open = json!({ "title": "Take bob's work", "head": "bob:loot", "base": "main" });
    let (status, body) = send(&server, &agent, Method::POST, "repos/alice/rocket/pulls", Some(open.clone())).await;
    assert_eq!(status, absent, "as a head that does not exist: {body}");
    let ensure = json!({ "head": "bob:loot", "base": "main", "title": "Take bob's work" });
    let (status, body) = send(&server, &agent, Method::POST, "repos/alice/rocket/pulls/ensure", Some(ensure)).await;
    assert!(status.is_client_error(), "{body}");
    // An unlimited token may; a limited one then still does not find it by that head.
    let (status, body) = send(&server, &admin, Method::POST, "repos/alice/rocket/pulls", Some(open)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, listed) = send(&server, &agent, Method::GET, "repos/alice/rocket/pulls?head=bob:loot", None).await;
    assert_eq!(listed, json!([]), "{listed}");
}

#[tokio::test]
async fn git_answers_a_limited_token_for_another_repository_as_if_it_did_not_exist() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "rocket", "public").await;
    bob.create_repo("bob", "plans", "private").await;
    let bobs = bob.create_token("bob", "admin").await;
    let read = json!({ "permission": "pull" });
    let (status, body) = send(&server, &bobs, Method::PUT, "repos/bob/plans/collaborators/alice", Some(read)).await;
    assert!(status.is_success(), "{body}");
    let admin = alice.create_token("owner", "admin").await;
    let limited = json!({ "name": "agent", "scope": "write", "repository": "alice/rocket", "rights": ["push"] });
    let (_, made) = send(&server, &admin, Method::POST, "user/tokens", Some(limited)).await;
    let agent = made["token"].as_str().unwrap().to_owned();
    // alice may read bob's repository but not write it; a token limited to rocket must not
    // learn that it exists: refusing the write (403) would say so.
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let advertise =
        |path: &str| http.get(server.url(&format!("/{path}/info/refs?service=git-receive-pack"))).basic_auth("alice", Some(&agent)).send();
    let missing = advertise("bob/nothing-here.git").await.unwrap().status();
    assert_eq!(missing, StatusCode::NOT_FOUND);
    assert_eq!(advertise("bob/plans.git").await.unwrap().status(), missing, "as a repository that does not exist");
    assert_eq!(advertise("alice/rocket.git").await.unwrap().status(), StatusCode::OK, "its own repository");
}
