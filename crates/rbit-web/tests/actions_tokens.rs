//! Task tokens as credentials (`docs/design/actions.md` §4.8, §11.3, §11.4): Git over HTTP,
//! LFS and the API accept them only while the task runs, only for its repository (others
//! public and read-only), at the granted permission, and the API only on its allowlist.
//!
//! The server side of the runner protocol (stream S) owns this file.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_core::{Timestamp, Visibility, actions::DefaultTokenPermission};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, state, template},
};

async fn send(fixture: &Fixture, method: Method, path: &str, authorization: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path).header(header::HOST, "git.example.com");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/vnd.git-lfs+json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = fixture.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// A running task of `alice/rocket` and its token.
async fn running_task(fixture: &Fixture) -> (String, i64, String) {
    let credential = fixture.runner(&["linux"]).await;
    seed(fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let spec = spec(&poll(fixture, &credential, Some("t")).await.1);
    (credential, spec.task_id.0, spec.token.expose().to_owned())
}

fn basic(token: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("x-access-token:{token}")))
}

#[tokio::test]
async fn git_accepts_a_task_token_only_while_its_task_runs() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let (credential, task, token) = running_task(&fixture).await;
    let fetch = "/alice/rocket/info/refs?service=git-upload-pack";
    let push = "/alice/rocket/info/refs?service=git-receive-pack";
    assert_eq!(send(&fixture, Method::GET, fetch, None, None).await.0, StatusCode::UNAUTHORIZED, "private");
    assert_eq!(send(&fixture, Method::GET, fetch, Some(&basic(&token)), None).await.0, StatusCode::OK);
    assert_eq!(send(&fixture, Method::GET, fetch, Some(&format!("Bearer {token}")), None).await.0, StatusCode::OK);
    assert_eq!(send(&fixture, Method::GET, push, Some(&basic(&token)), None).await.0, StatusCode::FORBIDDEN, "read is the default");

    // Another private repository stays hidden; a public one may be read.
    let bob = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    for (name, visibility) in [("secret", Visibility::Private), ("open", Visibility::Public)] {
        let input = rbit_services::CreateRepository { name: name.into(), description: String::new(), visibility };
        fixture.services.repositories.create(&bob, input).await.unwrap();
    }
    assert_eq!(
        send(&fixture, Method::GET, "/bob/secret/info/refs?service=git-upload-pack", Some(&basic(&token)), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&fixture, Method::GET, "/bob/open/info/refs?service=git-upload-pack", Some(&basic(&token)), None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&fixture, Method::GET, "/bob/open/info/refs?service=git-receive-pack", Some(&basic(&token)), None).await.0,
        StatusCode::FORBIDDEN,
        "never another repository's writer"
    );

    // The repository's default can grant write; the token is re-checked on every use.
    let repo = fixture.repo().await.repo.repo.id;
    fixture.database.actions_settings().set_token_permission(repo, DefaultTokenPermission::Write, Timestamp::now()).await.unwrap();
    assert_eq!(send(&fixture, Method::GET, push, Some(&basic(&token)), None).await.0, StatusCode::OK);
    fixture.database.actions_settings().set_enabled(repo, false, Timestamp::now()).await.unwrap();
    assert_eq!(send(&fixture, Method::GET, fetch, Some(&basic(&token)), None).await.0, StatusCode::UNAUTHORIZED, "Actions switched off");
    fixture.database.actions_settings().set_enabled(repo, true, Timestamp::now()).await.unwrap();

    let done = json!({"steps": [], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &credential, task, &done).await.0, StatusCode::OK);
    assert_eq!(send(&fixture, Method::GET, fetch, Some(&basic(&token)), None).await.0, StatusCode::UNAUTHORIZED, "dead with its task");
}

#[tokio::test]
async fn the_api_accepts_task_tokens_on_its_allowlist_only() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let (credential, task, token) = running_task(&fixture).await;
    let bearer = format!("Bearer {token}");
    let (status, json) = send(&fixture, Method::GET, "/api/v1/repos/alice/rocket", Some(&bearer), None).await;
    assert_eq!((status, json["full_name"].as_str()), (StatusCode::OK, Some("alice/rocket")));
    assert_eq!(send(&fixture, Method::GET, "/api/v1/repos/alice/rocket/branches", Some(&bearer), None).await.0, StatusCode::OK);
    let (status, json) = send(&fixture, Method::GET, "/api/v1/repos/alice/rocket/actions/artifacts", Some(&bearer), None).await;
    assert_eq!((status, json["total_count"].as_i64()), (StatusCode::OK, Some(0)), "{json}");
    for (method, path) in [
        (Method::DELETE, "/api/v1/repos/alice/rocket/actions/artifacts/1"),
        (Method::GET, "/api/v1/repos/alice/rocket/actions/runs"),
        (Method::GET, "/api/v1/user"),
        (Method::GET, "/api/v1/user/repos"),
        (Method::POST, "/api/v1/repos/alice/rocket/issues"),
        (Method::POST, "/api/v1/repos/alice/rocket/forks"),
        (Method::GET, "/api/v1/repos/alice/rocket/hooks"),
        (Method::DELETE, "/api/v1/repos/alice/rocket"),
    ] {
        let (status, json) = send(&fixture, method.clone(), path, Some(&bearer), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {json}");
    }
    let (status, _) =
        send(&fixture, Method::POST, "/api/v1/repos/alice/rocket/statuses/0123456789abcdef0123456789abcdef01234567", Some(&bearer), None)
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a read-only task posts no statuses");

    let done = json!({"steps": [], "outputs": {}, "result": "success"});
    state(&fixture, &credential, task, &done).await;
    assert_eq!(send(&fixture, Method::GET, "/api/v1/repos/alice/rocket", Some(&bearer), None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lfs_transfers_carry_the_task_credential() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let (_, _, token) = running_task(&fixture).await;
    let oid = "a".repeat(64);
    let batch = json!({"operation": "download", "objects": [{"oid": oid, "size": 3}]});
    let (status, json) = send(&fixture, Method::POST, "/alice/rocket.git/info/lfs/objects/batch", Some(&basic(&token)), Some(batch)).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let upload = json!({"operation": "upload", "objects": [{"oid": oid, "size": 3}]});
    let (status, _) =
        send(&fixture, Method::POST, "/alice/rocket.git/info/lfs/objects/batch", Some(&basic(&token)), Some(upload.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "uploads need contents: write");

    let repo = fixture.repo().await.repo.repo.id;
    fixture.database.actions_settings().set_token_permission(repo, DefaultTokenPermission::Write, Timestamp::now()).await.unwrap();
    let (status, json) = send(&fixture, Method::POST, "/alice/rocket.git/info/lfs/objects/batch", Some(&basic(&token)), Some(upload)).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["objects"][0]["actions"]["upload"]["header"]["Authorization"], basic(&token), "no transfer token: the task's own");
}

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(work: &std::path::Path, path: &str, contents: &str, message: &str) -> String {
    let file = work.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, contents).unwrap();
    git(work, &["add", "-A"]);
    git(work, &["commit", "-q", "-m", message]);
    git(work, &["rev-parse", "HEAD"])
}

/// A receive-pack request updating `refs/heads/main` from `old` to `new`, with the pack of
/// the commits between them.
fn receive_request(work: &std::path::Path, old: &str, new: &str) -> Vec<u8> {
    let mut child = std::process::Command::new("git")
        .args(["pack-objects", "--stdout", "--revs", "-q"])
        .current_dir(work)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), format!("{new}\n^{old}\n").as_bytes()).unwrap();
    let pack = child.wait_with_output().unwrap().stdout;
    let command = format!("{old} {new} refs/heads/main\0report-status\n");
    let mut body = format!("{:04x}{command}0000", command.len() + 4).into_bytes();
    body.extend(pack);
    body
}

/// `alice/rocket` with one commit on `main`, and that commit's id.
fn first_commit(repo: &rbit_services::RepoContext) -> String {
    let work = repo.disk_path.with_extension("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    let sha = commit(&work, "README.md", "# rocket\n", "Start");
    git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
    sha
}

#[tokio::test]
async fn writing_tasks_post_statuses_as_rbit_actions() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let repo = fixture.repo().await;
    let sha = first_commit(&repo);
    let (_, _, token) = running_task(&fixture).await;
    let bearer = format!("Bearer {token}");
    let post = |sha: &str| {
        let (path, bearer) = (format!("/api/v1/repos/alice/rocket/statuses/{sha}"), bearer.clone());
        let body = json!({"state": "success", "context": "deploy", "description": "Deployed", "target_url": "https://example.com/d/1"});
        let fixture = &fixture;
        async move { send(fixture, Method::POST, &path, Some(&bearer), Some(body)).await }
    };
    assert_eq!(post(&sha).await.0, StatusCode::FORBIDDEN, "read is the default");

    let write = DefaultTokenPermission::Write;
    fixture.database.actions_settings().set_token_permission(repo.repo.repo.id, write, Timestamp::now()).await.unwrap();
    let (status, json) = post(&sha).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!((json["state"].as_str(), json["context"].as_str()), (Some("success"), Some("deploy")));
    assert_eq!(json["creator"]["login"], "rbit-actions[bot]", "a workflow's status is rbit Actions', not a person's: {json}");
    let combined = fixture.services.statuses.combined(&repo, &sha).await.unwrap();
    assert_eq!(combined.statuses.len(), 1);
    assert_eq!(combined.statuses[0].creator, None);

    // The checks people pass still apply.
    let (status, json) = post("0123456789abcdef0123456789abcdef01234567").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert_eq!(post("main").await.0, StatusCode::UNPROCESSABLE_ENTITY, "a full commit id, as for people");
}

// Actions: releases from workflows.
#[tokio::test]
async fn writing_tasks_publish_releases_as_rbit_actions() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let repo = fixture.repo().await;
    first_commit(&repo);
    let (_, _, token) = running_task(&fixture).await;
    let bearer = format!("Bearer {token}");
    let releases = "/api/v1/repos/alice/rocket/releases";
    let new = json!({"tag_name": "v1.0.0", "target_commitish": "main", "name": "One", "draft": true});
    assert_eq!(
        send(&fixture, Method::POST, releases, Some(&bearer), Some(new.clone())).await.0,
        StatusCode::FORBIDDEN,
        "read is the default"
    );

    let write = DefaultTokenPermission::Write;
    fixture.database.actions_settings().set_token_permission(repo.repo.repo.id, write, Timestamp::now()).await.unwrap();
    let (status, release) = send(&fixture, Method::POST, releases, Some(&bearer), Some(new)).await;
    assert_eq!(status, StatusCode::CREATED, "{release}");
    assert_eq!(release["author"]["login"], "rbit-actions[bot]", "a workflow's release is rbit Actions', not a person's: {release}");
    let id = release["id"].as_i64().unwrap();
    let stored = fixture.services.releases.by_tag(&repo, "v1.0.0").await.unwrap().release;
    assert_eq!((stored.author, stored.source), (None, rbit_core::release::ReleaseSource::Actions));
    let tags = repo.reader.ref_snapshot().await.unwrap();
    assert!(tags.get("refs/tags/v1.0.0").is_some(), "the tag was created from main");

    // What publishing actions do next: find it by tag, attach a file, replace it, publish.
    let (status, found) = send(&fixture, Method::GET, &format!("{releases}/tags/v1.0.0"), Some(&bearer), None).await;
    assert_eq!((status, found["id"].as_i64()), (StatusCode::OK, Some(id)));
    let upload = format!("{releases}/{id}/assets?name=app.txt");
    let (status, asset) = send(&fixture, Method::POST, &upload, Some(&bearer), Some(json!("built"))).await;
    assert_eq!(status, StatusCode::CREATED, "{asset}");
    assert_eq!(asset["uploader"]["login"], "rbit-actions[bot]");
    let asset = asset["id"].as_i64().unwrap();
    assert_eq!(send(&fixture, Method::DELETE, &format!("{releases}/assets/{asset}"), Some(&bearer), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&fixture, Method::POST, &upload, Some(&bearer), Some(json!("rebuilt"))).await.0, StatusCode::CREATED);
    let (status, published) =
        send(&fixture, Method::PATCH, &format!("{releases}/{id}"), Some(&bearer), Some(json!({"draft": false}))).await;
    assert_eq!((status, published["draft"].as_bool()), (StatusCode::OK, Some(false)), "{published}");
    assert!(published["published_at"].is_string());

    // Never another repository's releases, even a public one it may read.
    let bob = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    let input = rbit_services::CreateRepository { name: "open".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&bob, input).await.unwrap();
    let theirs = json!({"tag_name": "v9", "target_commitish": "main"});
    let (status, json) = send(&fixture, Method::POST, "/api/v1/repos/bob/open/releases", Some(&bearer), Some(theirs)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");
    assert_eq!(send(&fixture, Method::DELETE, &format!("{releases}/{id}"), Some(&bearer), None).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn tasks_that_may_comment_write_as_rbit_actions() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let repo = fixture.repo().await;
    let input = rbit_services::issues::NewIssueInput { title: "Benchmarks".into(), ..Default::default() };
    let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    let (_, _, token) = running_task(&fixture).await;
    let bearer = format!("Bearer {token}");
    let path = format!("/api/v1/repos/alice/rocket/issues/{}/comments", issue.number.get());
    let body = json!({"body": "3% faster than main"});
    assert_eq!(
        send(&fixture, Method::POST, &path, Some(&bearer), Some(body.clone())).await.0,
        StatusCode::FORBIDDEN,
        "read is the default"
    );

    let write = DefaultTokenPermission::Write;
    fixture.database.actions_settings().set_token_permission(repo.repo.repo.id, write, Timestamp::now()).await.unwrap();
    let (status, json) = send(&fixture, Method::POST, &path, Some(&bearer), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["user"]["login"], "rbit-actions[bot]", "a workflow's comment is rbit Actions', not a person's: {json}");
    let id = rbit_core::CommentId(json["id"].as_i64().unwrap());
    let stored = fixture.database.issues().find_comment(issue.id, id).await.unwrap().unwrap();
    assert_eq!((stored.author, stored.source), (None, rbit_core::CommentSource::Actions));

    // Reading it back shows the bot too; no person may edit it.
    let thread = fixture.services.timeline.open(Some(&fixture.alice), repo.clone(), issue.number).await.unwrap();
    let (comment, author) = fixture.services.timeline.find_comment(&thread, id).await.unwrap();
    assert_eq!((comment.source, author.is_none()), (rbit_core::CommentSource::Actions, true));
    let edited = fixture.services.timeline.edit_comment(&fixture.alice, &thread, id, "mine now").await;
    assert!(matches!(edited, Err(rbit_services::ServiceError::Forbidden)), "{edited:?}");

    // A comment bot: list the thread, find its own comment, edit it, delete it. Only rbit
    // Actions' comments, never a person's.
    let alices = fixture.services.timeline.comment(&fixture.alice, &thread, "a person's").await.unwrap();
    let (status, listed) = send(&fixture, Method::GET, &path, Some(&bearer), None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let bodies: Vec<&str> = listed.as_array().unwrap().iter().filter_map(|comment| comment["body"].as_str()).collect();
    assert_eq!(bodies, ["3% faster than main", "a person's"]);
    let one = |id: i64| format!("/api/v1/repos/alice/rocket/issues/comments/{id}");
    assert_eq!(send(&fixture, Method::GET, &one(id.0), Some(&bearer), None).await.0, StatusCode::OK);
    let (status, json) = send(&fixture, Method::PATCH, &one(id.0), Some(&bearer), Some(json!({"body": "5% faster"}))).await;
    assert_eq!(
        (status, json["body"].as_str(), json["user"]["login"].as_str()),
        (StatusCode::OK, Some("5% faster"), Some("rbit-actions[bot]"))
    );
    let (status, json) = send(&fixture, Method::PATCH, &one(alices.id.0), Some(&bearer), Some(json!({"body": "rewritten"}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a person's comment: {json}");
    assert_eq!(send(&fixture, Method::DELETE, &one(alices.id.0), Some(&bearer), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(send(&fixture, Method::DELETE, &one(id.0), Some(&bearer), None).await.0, StatusCode::NO_CONTENT);

    // Another repository's threads stay out of reach.
    assert_eq!(
        send(&fixture, Method::POST, "/api/v1/repos/bob/open/issues/1/comments", Some(&bearer), Some(json!({"body": "hi"}))).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn task_pushes_land_unless_they_change_workflows() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let repo = fixture.repo().await;
    fixture
        .database
        .actions_settings()
        .set_token_permission(repo.repo.repo.id, DefaultTokenPermission::Write, Timestamp::now())
        .await
        .unwrap();
    let work = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap().disk_path.with_extension("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    let base = commit(&work, ".github/workflows/ci.yml", "on: push\n", "CI");
    git(&work, &["push", "-q", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
    let (_, _, token) = running_task(&fixture).await;

    let push = |body: Vec<u8>| {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/alice/rocket/git-receive-pack")
            .header(header::HOST, "git.example.com")
            .header(header::AUTHORIZATION, basic(&token))
            .header(header::CONTENT_TYPE, "application/x-git-receive-pack-request")
            .body(Body::from(body))
            .unwrap();
        let router = fixture.router.clone();
        async move {
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            (status, String::from_utf8_lossy(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).into_owned())
        }
    };

    let sneaky = commit(&work, ".github/workflows/ci.yml", "on: [push, pull_request]\n", "Widen CI");
    let (status, report) = push(receive_request(&work, &base, &sneaky)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(report.contains("ng refs/heads/main") && report.contains("workflow files"), "{report}");
    assert_eq!(repo.reader.commit_id("refs/heads/main".to_owned()).await.unwrap(), base, "nothing landed");

    git(&work, &["reset", "-q", "--hard", &base]);
    let code = commit(&work, "src/lib.rs", "fn main() {}\n", "Code");
    let (status, report) = push(receive_request(&work, &base, &code)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(report.contains("ok refs/heads/main"), "{report}");
    assert_eq!(repo.reader.commit_id("refs/heads/main".to_owned()).await.unwrap(), code, "the push landed through the replayed spool");
}
