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
    for (method, path) in [
        (Method::GET, "/api/v1/user"),
        (Method::GET, "/api/v1/user/repos"),
        (Method::GET, "/api/v1/repos/alice/rocket/issues"),
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
