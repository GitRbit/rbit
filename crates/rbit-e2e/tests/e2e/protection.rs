//! Branch protection over HTTP and SSH, end to end.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! These tests drive smart HTTP with the real `git` CLI, the settings pages and the REST
//! API. The same scenarios over SSH (with the real OpenSSH client) live in
//! `rbit-ssh/tests/protection.rs`, next to the SSH server they boot.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use rbit_e2e::{Client, Git, PASSWORD, StatusCode, TestServer};
use reqwest::Method;
use serde_json::{Value, json};

/// `refs/heads/{branch}`'s id on the server, if the branch exists.
fn remote_branch(git: &Git, dir: &Path, branch: &str) -> Option<String> {
    let listed = git.ok(dir, &["ls-remote", "origin", &format!("refs/heads/{branch}")]);
    listed.split_whitespace().next().map(str::to_owned)
}

/// Rewrites the last commit of the checked-out branch (a force push is needed to publish it).
fn rewrite(git: &Git, dir: &Path) -> String {
    git.ok(dir, &["commit", "--quiet", "--amend", "-m", "Rewritten history"]);
    git.ok(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// Adds the rule described by `fields` (checkbox names map to `"on"`); returns its page.
async fn add_rule(client: &Client, repo: &str, fields: &[(&str, &str)]) -> String {
    let response = client.post(&format!("/{repo}/settings/branches"), fields).await;
    response.expect_status(StatusCode::SEE_OTHER);
    let location = response.location().expect("redirects to the rule").to_owned();
    assert!(location.starts_with(&format!("/{repo}/settings/branches/")) && location.ends_with("?saved=1"), "{location}");
    location.split('?').next().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn protected_branches_over_http() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    server.sign_up("carol").await;
    alice.create_repo("alice", "demo", "public").await;
    for name in ["bob", "carol"] {
        let added = alice.post("/alice/demo/settings/collaborators", &[("username", name), ("permission", "write")]).await;
        added.expect_status(StatusCode::SEE_OTHER);
    }
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));

    // The settings pages: administrators only, Origin-checked forms, validation.
    alice.get("/alice/demo/settings").await.expect_contains(&["href=\"/alice/demo/settings/branches\"", "Branch protection"]);
    alice.get("/alice/demo/settings/branches").await.expect_status(StatusCode::OK).expect_contains(&["No protected branches"]);
    bob.get("/alice/demo/settings/branches").await.expect_status(StatusCode::NOT_FOUND);
    server.client().get("/alice/demo/settings/branches/new").await.expect_status(StatusCode::NOT_FOUND);
    bob.post("/alice/demo/settings/branches", &[("pattern", "main")]).await.expect_status(StatusCode::NOT_FOUND);
    let forged = alice.post_with("/alice/demo/settings/branches", &[("pattern", "main")], &[("Origin", "https://evil.example")]).await;
    forged.expect_status(StatusCode::FORBIDDEN);
    let invalid = alice.post("/alice/demo/settings/branches", &[("pattern", "a b")]).await;
    invalid.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["The pattern may not contain spaces"]);
    let main_rule = add_rule(
        &alice,
        "alice/demo",
        &[("pattern", "main"), ("deny_force_push", "on"), ("deny_deletion", "on"), ("required_approvals", "0")],
    )
    .await;
    alice.get(&main_rule).await.expect_contains(&["Matches:", "<span class=\"font-mono text-ink\">main</span>", "Danger zone"]);
    alice.get("/alice/demo/settings/branches").await.expect_contains(&[">main<", ">No force pushes<", ">No deletion<", "Matches main"]);
    alice.get("/alice/demo/settings/branches/999").await.expect_status(StatusCode::NOT_FOUND);
    alice.get("/alice/demo/settings/branches/abc").await.expect_status(StatusCode::NOT_FOUND);
    bob.get(&main_rule).await.expect_status(StatusCode::NOT_FOUND);

    // A force push to the protected main is rejected and main is unchanged.
    let alice_url = server.git_url_as("alice", "demo", "alice", PASSWORD);
    let work = git.clone(&alice_url);
    rewrite(&git, &work);
    let refused = git.fails(&work, &["push", "--force", "origin", "main"]);
    assert!(refused.contains("non-fast-forward"), "{refused}");
    assert_eq!(remote_branch(&git, &work, "main").as_deref(), Some(sample.head.as_str()));

    // Deleting it is rejected, with the reason.
    let refused = git.fails(&work, &["push", "origin", ":main"]);
    assert!(refused.contains("protected branch: deleting it is not allowed"), "{refused}");
    assert!(refused.contains("Rbit: refs/heads/main"), "the side band explains it: {refused}");
    assert_eq!(remote_branch(&git, &work, "main").as_deref(), Some(sample.head.as_str()));

    // A force push of an unprotected branch, in a separate push, still works.
    git.ok(&work, &["checkout", "--quiet", "feature"]);
    let rewritten = rewrite(&git, &work);
    git.ok(&work, &["push", "--quiet", "--force", "origin", "feature"]);
    assert_eq!(remote_branch(&git, &work, "feature"), Some(rewritten.clone()));

    // refs/pull/ belongs to Rbit.
    let refused = git.fails(&work, &["push", "origin", "feature:refs/pull/1/head"]);
    assert!(refused.contains("refs/pull/ is managed by Rbit"), "{refused}");

    // Pushes are all or nothing: the allowed update of feature is declined with the deletion.
    let next = git.commit_file(&work, "more.txt", "more\n");
    let refused = git.fails(&work, &["push", "origin", "feature", ":main"]);
    assert!(refused.contains("push declined: another ref in this push was refused"), "{refused}");
    assert_eq!(remote_branch(&git, &work, "feature"), Some(rewritten));
    git.ok(&work, &["push", "--quiet", "origin", "feature"]);
    assert_eq!(remote_branch(&git, &work, "feature"), Some(next));

    // Restricted pushers: carol may push to main, bob (a writer too) may not.
    let restricted =
        [("pattern", "main"), ("deny_force_push", "on"), ("deny_deletion", "on"), ("restrict_pushers", "on"), ("users", "carol")];
    alice.post(&main_rule, &restricted).await.expect_redirect(&format!("{main_rule}?saved=1"));
    let bobs = git.clone(&server.git_url_as("alice", "demo", "bob", PASSWORD));
    git.commit_file(&bobs, "bob.txt", "bob\n");
    let refused = git.fails(&bobs, &["push", "origin", "main"]);
    assert!(refused.contains("protected branch: you are not allowed to push to it"), "{refused}");
    assert!(!refused.contains("carol"), "refusals never say who is allowed");
    let carols = git.clone(&server.git_url_as("alice", "demo", "carol", PASSWORD));
    let carol_head = git.commit_file(&carols, "carol.txt", "carol\n");
    git.ok(&carols, &["push", "--quiet", "origin", "main"]);
    assert_eq!(remote_branch(&git, &carols, "main"), Some(carol_head.clone()));

    // Release branches need pull requests: creating one is allowed, pushing to it is not.
    add_rule(&alice, "alice/demo", &[("pattern", "release/*"), ("require_pull_request", "on"), ("required_approvals", "1")]).await;
    git.ok(&carols, &["push", "--quiet", "origin", "main:release/1.0"]);
    git.ok(&carols, &["checkout", "--quiet", "-b", "release/1.0", "origin/main"]);
    git.commit_file(&carols, "fix.txt", "fix\n");
    let refused = git.fails(&carols, &["push", "origin", "release/1.0"]);
    assert!(refused.contains("protected branch: changes must go through a pull request"), "{refused}");
    alice.get("/alice/demo/settings/branches").await.expect_contains(&[">1 approval<", "Matches release/1.0"]);

    // The owner is held to the rules too, until the rule lets administrators bypass it.
    git.ok(&work, &["fetch", "--quiet", "origin"]);
    git.ok(&work, &["checkout", "--quiet", "-B", "main", "origin/main"]);
    git.commit_file(&work, "admin.txt", "admin\n");
    let refused = git.fails(&work, &["push", "origin", "main"]);
    assert!(refused.contains("you are not allowed to push to it"), "{refused}");
    let bypass = [restricted.as_slice(), &[("allow_admin_bypass", "on")]].concat();
    alice.post(&main_rule, &bypass).await.expect_status(StatusCode::SEE_OTHER);
    // A real rewrite: replace the last commit that is already on the server.
    git.ok(&work, &["reset", "--quiet", "--hard", "origin/main"]);
    let rewritten_main = rewrite(&git, &work);
    git.ok(&work, &["push", "--quiet", "--force", "origin", "main"]);
    assert_eq!(remote_branch(&git, &work, "main"), Some(rewritten_main));
    alice.get("/admin/audit").await.expect_contains(&["Bypassed branch protection", "refs/heads/main", "Added a branch protection rule"]);
    // Bob still cannot.
    git.fails(&bobs, &["push", "--force", "origin", "main"]);

    // Deleting a rule needs the confirmation box.
    alice
        .post(&format!("{main_rule}/delete"), &[])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["Tick the box"]);
    alice.post(&format!("{main_rule}/delete"), &[("confirm", "on")]).await.expect_redirect("/alice/demo/settings/branches?deleted=1");
    alice.get(&main_rule).await.expect_status(StatusCode::NOT_FOUND);
    git.ok(&bobs, &["push", "--quiet", "--force", "origin", "main"]);
    assert_ne!(remote_branch(&git, &bobs, "main"), Some(carol_head));
}

#[tokio::test]
async fn clients_without_report_status_get_a_403() {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    add_rule(&alice, "alice/demo", &[("pattern", "main"), ("deny_deletion", "on")]).await;

    let zero = "0".repeat(40);
    let line = format!("{} {zero} refs/heads/main", sample.head);
    let mut body = format!("{:04x}{line}", line.len() + 4).into_bytes();
    body.extend_from_slice(b"0000");
    let content_type = "application/x-git-receive-pack-request";
    let refused = server.client().post_basic("/alice/demo.git/git-receive-pack", "alice", PASSWORD, content_type, body).await;
    refused.expect_status(StatusCode::FORBIDDEN).expect_contains(&["refs/heads/main: protected branch: deleting it is not allowed"]);

    // A malformed command list is a 400, not a git error.
    let garbage = server.client().post_basic("/alice/demo.git/git-receive-pack", "alice", PASSWORD, content_type, b"zzzz".to_vec()).await;
    garbage.expect_status(StatusCode::BAD_REQUEST).expect_contains(&["malformed push request"]);
}

#[tokio::test]
async fn mirrors_are_refused_before_protection_is_considered() {
    let server = TestServer::start_with(|config| config.mirrors.allow_private_addresses = true);
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "upstream", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "upstream", "alice", PASSWORD));
    let upstream = server.git_url("alice", "upstream");
    alice
        .post("/new/mirror", &[("owner", "alice"), ("url", &upstream), ("name", "copy"), ("visibility", "public"), ("interval", "60")])
        .await
        .expect_redirect("/alice/copy");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !alice.get("/alice/copy/branches").await.contains("feature") {
        assert!(Instant::now() < deadline, "the mirror never fetched");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    add_rule(&alice, "alice/copy", &[("pattern", "*"), ("restrict_pushers", "on")]).await;

    let checkout = git.clone(&server.git_url_as("alice", "copy", "alice", PASSWORD));
    git.commit_file(&checkout, "local.txt", "local\n");
    let refused = git.fails(&checkout, &["push", "origin", "main"]);
    assert!(refused.contains("This repository is a mirror; push to its upstream instead"), "{refused}");
    assert!(!refused.contains("protected branch"), "{refused}");
}

// ------------------------------------------------------------------ REST API

/// A plain API client: no cookies, a bearer token.
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

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (StatusCode, Option<String>, Value) {
        let mut request =
            self.http.request(method, format!("{}{path}", self.base)).header("authorization", format!("Bearer {}", self.token));
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        let response = request.send().await.expect("the API answers");
        let status = response.status();
        let location = response.headers().get("location").and_then(|value| value.to_str().ok()).map(str::to_owned);
        let text = response.text().await.expect("read the body");
        let json = if text.is_empty() { Value::Null } else { serde_json::from_str(&text).expect("API bodies are JSON") };
        (status, location, json)
    }
}

#[tokio::test]
async fn rules_through_the_api() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    let dave = server.sign_up("dave").await;
    alice.create_repo("alice", "demo", "public").await;
    alice.create_repo("alice", "vault", "private").await;
    alice
        .post("/alice/demo/settings/collaborators", &[("username", "bob"), ("permission", "write")])
        .await
        .expect_status(StatusCode::SEE_OTHER);
    let admin = Api::new(&server, &alice.create_token("admin", "admin").await);
    let write = Api::new(&server, &alice.create_token("write", "write").await);
    let bobs = Api::new(&server, &bob.create_token("admin", "admin").await);
    let daves = Api::new(&server, &dave.create_token("admin", "admin").await);
    let base = "/repos/alice/demo/branch_protections";

    let (status, _, list) = admin.send(Method::GET, base, None).await;
    assert_eq!((status, list), (StatusCode::OK, json!([])));
    let body = json!({ "pattern": "main", "require_pull_request": true, "required_approvals": 2, "restrict_pushers": true, "push_allowlist_users": ["bob"] });
    let (status, location, rule) = admin.send(Method::POST, base, Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{rule}");
    let id = rule["id"].as_i64().unwrap();
    assert_eq!(location.as_deref(), Some(format!("{}/api/v1{base}/{id}", server.base_url()).as_str()));
    assert_eq!(rule["url"], location.unwrap());
    assert_eq!((rule["pattern"].as_str(), rule["required_approvals"].as_u64()), (Some("main"), Some(2)));
    assert_eq!((rule["deny_force_push"].as_bool(), rule["allow_admin_bypass"].as_bool()), (Some(true), Some(false)), "defaults");
    assert_eq!(rule["push_allowlist_users"], json!(["bob"]));
    assert_eq!(rule["push_allowlist_teams"], json!([]));
    assert!(rule["created_at"].as_str().is_some_and(|at| at.ends_with('Z')));

    let one = format!("{base}/{id}");
    let (status, _, shown) = admin.send(Method::GET, &one, None).await;
    assert_eq!((status, &shown), (StatusCode::OK, &rule));
    let (status, _, changed) = admin.send(Method::PATCH, &one, Some(json!({ "pattern": "trunk", "allow_admin_bypass": true }))).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        (changed["pattern"].as_str(), changed["allow_admin_bypass"].as_bool(), changed["required_approvals"].as_u64()),
        (Some("trunk"), Some(true), Some(2))
    );
    assert_eq!(changed["push_allowlist_users"], json!(["bob"]), "absent fields keep their value");

    // Validation errors name the field.
    for (body, field) in [
        (json!({ "pattern": "a b" }), "pattern"),
        (json!({}), "pattern"),
        (json!({ "pattern": "trunk" }), "pattern"),
        (json!({ "pattern": "x", "required_approvals": 3 }), "required_approvals"),
        (json!({ "pattern": "x", "restrict_pushers": true, "push_allowlist_users": ["nobody"] }), "push_allowlist_users"),
        (json!({ "pattern": "x", "restrict_pushers": true, "push_allowlist_teams": ["core"] }), "push_allowlist_teams"),
    ] {
        let (status, _, error) = admin.send(Method::POST, base, Some(body.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {error}");
        assert_eq!(error["errors"][0]["field"], field, "{body}: {error}");
    }

    // Narrower scopes and non-administrators get 403; callers who cannot see the repository 404.
    assert_eq!(write.send(Method::GET, base, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(bobs.send(Method::GET, base, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(bobs.send(Method::DELETE, &one, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(daves.send(Method::GET, base, None).await.0, StatusCode::FORBIDDEN, "a public repository is visible");
    assert_eq!(daves.send(Method::GET, "/repos/alice/vault/branch_protections", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(admin.send(Method::GET, &format!("{base}/999"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(admin.send(Method::GET, &format!("{base}/nope"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(admin.send(Method::GET, &format!("/repos/alice/vault/branch_protections/{id}"), None).await.0, StatusCode::NOT_FOUND);

    let (status, _, _) = admin.send(Method::DELETE, &one, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(admin.send(Method::GET, &one, None).await.0, StatusCode::NOT_FOUND);
    let (_, _, document) = admin.send(Method::GET, "/openapi.json", None).await;
    assert!(document["paths"]["/repos/{owner}/{repo}/branch_protections/{id}"]["patch"].is_object());
}
