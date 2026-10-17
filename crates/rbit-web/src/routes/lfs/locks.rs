//! The LFS locks API: `…/info/lfs/locks` (list, create), `…/locks/verify` (before a
//! push) and `…/locks/{id}/unlock`. The rules live in `LfsService` (`lfs::locks`).

use axum::{
    Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
};
use rbit_core::lfs::LfsLock;
use rbit_services::lfs::locks::{LockCreated, LockQuery};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{LfsError, authorize, json_response, parse_json};
use crate::{AppState, client_ip::Client};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/info/lfs/locks", get(list).post(create))
        .route("/{owner}/{repo}/info/lfs/locks/verify", post(verify))
        .route("/{owner}/{repo}/info/lfs/locks/{id}/unlock", post(unlock))
}

/// A lock as the API shows it.
fn lock_json(lock: &LfsLock) -> Value {
    json!({
        "id": lock.id.0.to_string(),
        "path": lock.path,
        "locked_at": lock.locked_at.iso8601(),
        "owner": { "name": lock.owner.name },
    })
}

fn locks_json(locks: &[LfsLock]) -> Vec<Value> {
    locks.iter().map(lock_json).collect()
}

#[derive(Debug, Deserialize)]
struct ListParams {
    path: Option<String>,
    id: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

/// `GET /{owner}/{repo}/info/lfs/locks[?path=&id=&cursor=&limit=]`
async fn list(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Query(params): Query<ListParams>,
    client: Client,
    headers: HeaderMap,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_repository_token()?;
    let query = LockQuery { path: params.path, id: params.id, cursor: params.cursor, limit: params.limit };
    let page = state.services.lfs.list_locks(&authorized.ctx, &query).await?;
    let mut body = json!({ "locks": locks_json(&page.locks) });
    if let Some(cursor) = page.next_cursor {
        body["next_cursor"] = json!(cursor);
    }
    Ok(json_response(StatusCode::OK, &body))
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    path: String,
}

/// `POST /{owner}/{repo}/info/lfs/locks`: `201` with the new lock, `409` with the
/// existing one.
async fn create(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_repository_token()?;
    authorized.require_write(&state, false).await?;
    let request: CreateRequest = parse_json(&body)?;
    match state.services.lfs.create_lock(&authorized.ctx, authorized.user.as_ref(), &request.path).await? {
        LockCreated::Created(lock) => Ok(json_response(StatusCode::CREATED, &json!({ "lock": lock_json(&lock) }))),
        LockCreated::Exists(lock) => {
            Ok(json_response(StatusCode::CONFLICT, &json!({ "lock": lock_json(&lock), "message": "already created lock" })))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct VerifyRequest {
    cursor: Option<String>,
    limit: Option<u32>,
}

/// `POST /{owner}/{repo}/info/lfs/locks/verify`: the caller's locks and everyone else's.
async fn verify(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_repository_token()?;
    authorized.require_write(&state, false).await?;
    let request: VerifyRequest = if body.is_empty() { VerifyRequest::default() } else { parse_json(&body)? };
    let lfs = &state.services.lfs;
    let verified = lfs.verify_locks(&authorized.ctx, authorized.user.as_ref(), request.cursor.as_deref(), request.limit).await?;
    let mut body = json!({ "ours": locks_json(&verified.ours), "theirs": locks_json(&verified.theirs) });
    if let Some(cursor) = verified.next_cursor {
        body["next_cursor"] = json!(cursor);
    }
    Ok(json_response(StatusCode::OK, &body))
}

#[derive(Debug, Default, Deserialize)]
struct UnlockRequest {
    #[serde(default)]
    force: bool,
}

/// `POST /{owner}/{repo}/info/lfs/locks/{id}/unlock`
async fn unlock(
    State(state): State<AppState>,
    Path((owner, repo, id)): Path<(String, String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_repository_token()?;
    authorized.require_write(&state, false).await?;
    let request: UnlockRequest = if body.is_empty() { UnlockRequest::default() } else { parse_json(&body)? };
    let lock = state.services.lfs.unlock(&authorized.ctx, authorized.user.as_ref(), &id, request.force).await?;
    Ok(json_response(StatusCode::OK, &json!({ "lock": lock_json(&lock) })))
}
