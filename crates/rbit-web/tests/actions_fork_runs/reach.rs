//! What a task token reaches over Git, LFS and the API: one request per kind of write, and
//! one read as the control that the token works at all.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::world::World;

/// The answers a token got, by what it tried.
#[derive(Debug, PartialEq, Eq)]
pub struct Reach {
    pub fetch: StatusCode,
    pub push: StatusCode,
    pub lfs_upload: StatusCode,
    pub status: StatusCode,
    pub release: StatusCode,
    pub comment: StatusCode,
}

impl Reach {
    /// Reads its repository and is refused every write.
    pub fn read_only(&self) -> bool {
        self.fetch == StatusCode::OK
            && [self.push, self.lfs_upload, self.status, self.release, self.comment].iter().all(|status| *status == StatusCode::FORBIDDEN)
    }

    /// Every write went through.
    pub fn writes(&self) -> bool {
        self.fetch == StatusCode::OK
            && [self.push, self.lfs_upload].iter().all(|status| *status == StatusCode::OK)
            && [self.status, self.release, self.comment].iter().all(|status| *status == StatusCode::CREATED)
    }
}

async fn send(world: &World, method: Method, path: &str, authorization: &str, body: Option<Value>) -> StatusCode {
    let mut request = Request::builder().method(method).uri(path).header(header::HOST, "git.example.com");
    request = request.header(header::AUTHORIZATION, authorization);
    let body = match body {
        Some(body) => {
            let kind = if path.contains("/info/lfs/") { "application/vnd.git-lfs+json" } else { "application/json" };
            request = request.header(header::CONTENT_TYPE, kind).header(header::ACCEPT, kind);
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = world.fixture.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let _ = to_bytes(response.into_body(), usize::MAX).await;
    status
}

/// Tries `token` against `alice/rocket`: commit `sha` takes the status, issue `issue` the
/// comment, and `tag` names the release.
pub async fn reach(world: &World, token: &str, sha: &str, issue: u32, tag: &str) -> Reach {
    let basic = format!("Basic {}", STANDARD.encode(format!("x-access-token:{token}")));
    let bearer = format!("Bearer {token}");
    let fetch = send(world, Method::GET, "/alice/rocket/info/refs?service=git-upload-pack", &basic, None).await;
    let push = send(world, Method::GET, "/alice/rocket/info/refs?service=git-receive-pack", &basic, None).await;
    let upload = json!({"operation": "upload", "objects": [{"oid": "b".repeat(64), "size": 3}]});
    let lfs_upload = send(world, Method::POST, "/alice/rocket.git/info/lfs/objects/batch", &basic, Some(upload)).await;
    let status_body = json!({"state": "success", "context": "fork", "description": "green"});
    let status = send(world, Method::POST, &format!("/api/v1/repos/alice/rocket/statuses/{sha}"), &bearer, Some(status_body)).await;
    let release_body = json!({"tag_name": tag, "target_commitish": "main", "name": tag});
    let release = send(world, Method::POST, "/api/v1/repos/alice/rocket/releases", &bearer, Some(release_body)).await;
    let comment_path = format!("/api/v1/repos/alice/rocket/issues/{issue}/comments");
    let comment = send(world, Method::POST, &comment_path, &bearer, Some(json!({"body": "from a workflow"}))).await;
    Reach { fetch, push, lfs_upload, status, release, comment }
}
