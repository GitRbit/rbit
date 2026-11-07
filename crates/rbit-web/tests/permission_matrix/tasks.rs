//! Runner-issued credentials: a running task's token opens its own repository at its
//! grant and every other repository as an anonymous visitor would (public ones read-only),
//! over Git smart HTTP and the API, and its own repository refuses its writes once archived.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_core::{RepoAccess, Timestamp, actions::DefaultTokenPermission};
use rbit_services::{CreateRepository, orgs::CreateOrganization};
use rbit_storage::NewUser;
use serde_json::json;
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, template},
    permission_world::{SHAPES, Shape, git},
};

async fn send(fixture: &Fixture, method: Method, path: &str, authorization: &str, body: Option<serde_json::Value>) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(path).header(header::AUTHORIZATION, authorization);
    if body.is_some() {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    let body = body.map_or_else(Body::empty, |body| Body::from(body.to_string()));
    let response = fixture.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Deletes `branch` of `owner/name` over Git smart HTTP; the push report.
async fn delete_branch(fixture: &Fixture, full_name: &str, basic: &str, head: &str, branch: &str) -> (StatusCode, String) {
    let mut body = rbit_transport::pkt_line::encode(&format!("{head} {} {branch}\0report-status\n", "0".repeat(40)));
    body.extend_from_slice(b"0000");
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/{full_name}.git/git-receive-pack"))
        .header(header::AUTHORIZATION, basic)
        .header(header::CONTENT_TYPE, "application/x-git-receive-pack-request")
        .body(Body::from(body))
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Every other shape of the shared world, created beside the task's private `alice/rocket`.
async fn other_repositories(fixture: &Fixture) {
    let new = NewUser {
        name: "olga".parse().unwrap(),
        email: "olga@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    let olga = fixture.database.accounts().create_user(new).await.unwrap();
    fixture.services.orgs.create(&olga, CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() }).await.unwrap();
    for shape in SHAPES {
        let creator = if shape.organization { &olga } else { &fixture.alice };
        let visibility = if shape.public { rbit_core::Visibility::Public } else { rbit_core::Visibility::Private };
        let input = CreateRepository { name: shape.name().to_owned(), description: String::new(), visibility };
        fixture.services.repositories.create_in(creator, shape.owner(), input).await.unwrap();
        if shape.archived {
            let ctx = fixture.services.repositories.open(Some(creator), shape.owner(), shape.name()).await.unwrap();
            fixture.services.repo_settings.set_archived(creator, &ctx, true).await.unwrap();
        }
    }
}

/// A task's access to a repository that is not its own: what anonymous visitors get.
fn foreign(shape: Shape) -> RepoAccess {
    if shape.public { RepoAccess::Read } else { RepoAccess::None }
}

#[tokio::test]
async fn task_tokens_follow_their_grant_and_the_archive_guard() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let rocket = fixture.repo().await;
    let seed_dir = rocket.disk_path.with_extension("seed");
    std::fs::create_dir_all(&seed_dir).unwrap();
    git(&seed_dir, &["init", "-q", "-b", "main"]);
    std::fs::write(seed_dir.join("README.md"), "# rocket\n").unwrap();
    git(&seed_dir, &["add", "README.md"]);
    git(&seed_dir, &["commit", "-q", "-m", "seed"]);
    let push =
        ["push", "-q", rocket.disk_path.to_str().unwrap(), "HEAD:refs/heads/main", "HEAD:refs/heads/task-a", "HEAD:refs/heads/task-b"];
    git(&seed_dir, &push);
    let head = git(&seed_dir, &["rev-parse", "HEAD"]);
    other_repositories(&fixture).await;

    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let token = spec(&poll(&fixture, &credential, Some("t")).await.1).token.expose().to_owned();
    let (bearer, basic) = (format!("Bearer {token}"), format!("Basic {}", STANDARD.encode(format!("x-access-token:{token}"))));
    let write = DefaultTokenPermission::Write;
    fixture.database.actions_settings().set_token_permission(rocket.repo.repo.id, write, Timestamp::now()).await.unwrap();

    for shape in SHAPES {
        let (full_name, access) = (shape.full_name(), foreign(shape));
        let found = |status| if access.can_read() { status } else { StatusCode::NOT_FOUND };
        let fetch = send(&fixture, Method::GET, &format!("/{full_name}.git/info/refs?service=git-upload-pack"), &basic, None).await;
        assert_eq!(fetch.0, found(StatusCode::OK), "fetch {full_name}");
        let push = send(&fixture, Method::GET, &format!("/{full_name}.git/info/refs?service=git-receive-pack"), &basic, None).await;
        assert_eq!(push.0, found(StatusCode::FORBIDDEN), "push {full_name}");
        assert_eq!(send(&fixture, Method::GET, &format!("/api/v1/repos/{full_name}"), &bearer, None).await.0, found(StatusCode::OK));
        let status = json!({"state": "success", "context": "task"});
        let posted = send(&fixture, Method::POST, &format!("/api/v1/repos/{full_name}/statuses/{head}"), &bearer, Some(status)).await;
        assert_eq!(posted.0, found(StatusCode::FORBIDDEN), "status {full_name}: {}", posted.1);
    }

    // Its own repository at its grant, until the repository is archived.
    let status = json!({"state": "success", "context": "task"});
    let own = format!("/api/v1/repos/alice/rocket/statuses/{head}");
    assert_eq!(send(&fixture, Method::POST, &own, &bearer, Some(status.clone())).await.0, StatusCode::CREATED);
    let (code, report) = delete_branch(&fixture, "alice/rocket", &basic, &head, "refs/heads/task-a").await;
    assert_eq!(code, StatusCode::OK);
    assert!(report.contains("ok refs/heads/task-a"), "{report}");

    let ctx = fixture.repo().await;
    assert!(fixture.services.repo_settings.set_archived(&fixture.alice, &ctx, true).await.unwrap());
    let (code, body) = send(&fixture, Method::POST, &own, &bearer, Some(status)).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("archived"), "{body}");
    let (code, report) = delete_branch(&fixture, "alice/rocket", &basic, &head, "refs/heads/task-b").await;
    assert_eq!(code, StatusCode::OK);
    assert!(report.contains("ng refs/heads/task-b") && report.contains("archived"), "{report}");
    assert!(git(&rocket.disk_path, &["for-each-ref", "refs/heads/task-b"]).contains("task-b"));
}
