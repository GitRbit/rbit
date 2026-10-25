//! The Git LFS API: `/{owner}/{repo}[.git]/info/lfs/…` (batch, basic transfers, locks).
//!
//! Milestone 3 workstream L (Git LFS) owns this file and [`locks`] and [`files`].
//!
//! Clients derive the endpoint from the remote URL, with or without `.git`; the `{repo}`
//! segment takes both. Like smart HTTP these routes run without the page layers (no
//! sessions, CSRF check, compression or timeout): bodies stream, and authentication is
//! per request:
//!
//! * HTTP Basic with a password or personal access token, exactly as for Git
//!   ([`git_http::authenticate`]: throttled, two-factor accounts need a token, a token's
//!   scope caps access), or none for public repositories;
//! * `Authorization: Bearer lfs1.…`, the short-lived signed header the batch API and
//!   `git-lfs-authenticate` (SSH) hand out. Invalid ones count as failed attempts;
//! * a workflow task's token (`rbat_…`), as for Git: its own repository at its grant
//!   (downloads; uploads with `contents: write`), other public ones read-only. The batch
//!   API hands a task back its own header for the transfers instead of a transfer token,
//!   so every transfer re-checks that the task still runs (Actions: S).
//!
//! Downloads need read access, uploads and locks write access; pull mirrors refuse
//! uploads. Hidden repositories answer `404` (`401` to anonymous callers, so clients ask
//! for credentials). Everything answers `404` when `[lfs] enabled` is off. Errors are
//! JSON (`{"message": …}`) with the LFS media type.

mod files;
mod locks;

pub(super) use files::{lfs_blob, lfs_raw};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::TryStreamExt;
use rbit_core::{
    User,
    lfs::{LfsOid, LfsOperation},
};
use rbit_services::{
    RepoContext, ServiceError, ThrottleScope,
    lfs::{BatchOutcome, BatchRequestObject, IssuedToken, TokenSubject, tokens::looks_like_token},
    mirrors,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::io::{ReaderStream, StreamReader};

use super::git_http::{self, Denied, GitCaller};
use crate::{AppState, client_ip::Client};

/// The media type of every LFS request and response body.
pub(crate) const LFS_JSON: &str = "application/vnd.git-lfs+json";
/// Upper bound for JSON request bodies (a batch of 1000 objects is about 100 KiB).
const MAX_JSON_BYTES: usize = 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/info/lfs/objects/batch", post(batch))
        .route("/{owner}/{repo}/info/lfs/objects/{oid}", get(download).put(upload))
        .route("/{owner}/{repo}/info/lfs/objects/{oid}/verify", post(verify))
        .merge(locks::routes())
        // Only buffered (JSON) bodies are bounded; uploads stream past this.
        .layer(DefaultBodyLimit::max(MAX_JSON_BYTES))
}

// ------------------------------------------------------------------ errors

/// A refused LFS request, answered as `{"message": …}` with the LFS media type.
#[derive(Debug)]
pub(crate) enum LfsError {
    /// `401`: ask for credentials.
    Challenge,
    Status(StatusCode, String),
    RateLimited {
        retry_after_seconds: u64,
    },
}

impl LfsError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self::Status(status, message.into())
    }

    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found")
    }
}

impl IntoResponse for LfsError {
    fn into_response(self) -> Response {
        match self {
            Self::Challenge => {
                let mut response = json_response(StatusCode::UNAUTHORIZED, &json!({ "message": "Credentials needed" }));
                let challenge = HeaderValue::from_static("Basic realm=\"rbit\", charset=\"UTF-8\"");
                response.headers_mut().insert("lfs-authenticate", challenge.clone());
                response.headers_mut().insert(header::WWW_AUTHENTICATE, challenge);
                response
            }
            Self::Status(status, message) => json_response(status, &json!({ "message": message })),
            Self::RateLimited { retry_after_seconds } => {
                let message = format!("too many authentication attempts; try again in {retry_after_seconds} seconds");
                let mut response = json_response(StatusCode::TOO_MANY_REQUESTS, &json!({ "message": message }));
                response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(retry_after_seconds));
                response
            }
        }
    }
}

impl From<Denied> for LfsError {
    fn from(denied: Denied) -> Self {
        match denied {
            Denied::Challenge => Self::Challenge,
            Denied::RateLimited { retry_after_seconds } => Self::RateLimited { retry_after_seconds },
            Denied::NotFound => Self::not_found(),
            Denied::Internal => Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
            Denied::Suspended => Self::new(StatusCode::FORBIDDEN, "this account has been suspended"),
            Denied::TokenRequired => Self::new(
                StatusCode::FORBIDDEN,
                "two-factor authentication is enabled for this account, so its password cannot be used with Git; \
                 use a personal access token as the password",
            ),
            Denied::Forbidden => Self::new(StatusCode::FORBIDDEN, "you do not have permission to push to this repository"),
            Denied::Mirror => Self::new(StatusCode::FORBIDDEN, mirrors::PUSH_REFUSED),
        }
    }
}

impl From<ServiceError> for LfsError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::not_found(),
            ServiceError::Forbidden => Self::new(StatusCode::FORBIDDEN, error.to_string()),
            ServiceError::Invalid(message) => Self::new(StatusCode::UNPROCESSABLE_ENTITY, message),
            ServiceError::Conflict(_) => Self::new(StatusCode::CONFLICT, error.to_string()),
            ServiceError::RateLimited { retry_after_seconds } => Self::RateLimited { retry_after_seconds },
            other => {
                tracing::error!(error = %other, "LFS request failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        }
    }
}

pub(crate) fn json_response(status: StatusCode, body: &Value) -> Response {
    (status, [(header::CONTENT_TYPE, LFS_JSON)], body.to_string()).into_response()
}

/// Parses a JSON request body; malformed JSON is `422`, as the LFS API specifies.
pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, LfsError> {
    serde_json::from_slice(body).map_err(|error| LfsError::new(StatusCode::UNPROCESSABLE_ENTITY, format!("invalid request: {error}")))
}

// ------------------------------------------------------------------ authorization

/// An LFS request's repository and who makes it.
#[derive(Debug)]
pub(crate) struct Authorized {
    pub(crate) ctx: RepoContext,
    pub(crate) user: Option<User>,
    /// Who tokens issued for this request act for.
    pub(crate) subject: TokenSubject,
    /// Set when authorised by a transfer token: the one object, and its size, it covers.
    pub(crate) object: Option<(LfsOid, u64)>,
    /// Whether any credential was presented (anonymous callers get `401` instead of `403`).
    authenticated: bool,
    // Actions: S.
    /// Authorised by a workflow task's token.
    task: bool,
}

impl Authorized {
    /// Requires write access (and, for uploads, a repository that is not a mirror).
    pub(crate) async fn require_write(&self, state: &AppState, upload: bool) -> Result<(), LfsError> {
        if !self.ctx.access.can_write() {
            return Err(if self.authenticated {
                LfsError::new(StatusCode::FORBIDDEN, "you do not have permission to push to this repository")
            } else {
                LfsError::Challenge
            });
        }
        if upload && state.services.mirrors.is_mirror(self.ctx.repo.repo.id).await? {
            return Err(LfsError::new(StatusCode::FORBIDDEN, mirrors::PUSH_REFUSED));
        }
        Ok(())
    }

    /// For transfer endpoints: a transfer token must cover exactly `oid`.
    fn require_object(&self, oid: &LfsOid) -> Result<(), LfsError> {
        match &self.object {
            Some((covered, _)) if covered != oid => Err(LfsError::new(StatusCode::FORBIDDEN, "this token is for another object")),
            _ => Ok(()),
        }
    }

    /// Object transfer credentials cannot mint new grants or inspect/change locks.
    fn require_repository_token(&self) -> Result<(), LfsError> {
        if self.object.is_some() {
            return Err(LfsError::new(StatusCode::FORBIDDEN, "this token is only for an object transfer"));
        }
        Ok(())
    }
}

/// Resolves the credentials and opens `owner/repo` (old names too) for them.
pub(crate) async fn authorize(
    state: &AppState,
    client: &Client,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
) -> Result<Authorized, LfsError> {
    let lfs = &state.services.lfs;
    if !lfs.enabled() {
        return Err(LfsError::not_found());
    }
    if let Some(token) = bearer_token(headers).filter(|token| looks_like_token(token)) {
        let ip = client.ip().map(|ip| ip.to_canonical().to_string()).unwrap_or_default();
        state.services.throttle.check_client(ThrottleScope::GitAuth, &ip)?;
        let Some(grant) = lfs.verify_token(token) else {
            state.services.throttle.record_client_failure(ThrottleScope::GitAuth, &ip);
            return Err(LfsError::Challenge);
        };
        let (subject, object) = (grant.subject, grant.object.clone());
        let granted = lfs.open_granted(grant, owner, repo).await?;
        return Ok(Authorized { ctx: granted.ctx, user: granted.user, subject, object, authenticated: true, task: false });
    }

    let caller = git_http::authenticate(state, client, headers).await?;
    // Actions: S. A task opens its own repository at its grant, others only if public.
    if let (Some(GitCaller::Task(grant)), Some(actions)) = (&caller, &state.services.actions) {
        let ctx = actions.open_for_task(grant, owner, repo).await?;
        return Ok(Authorized { ctx, user: None, subject: TokenSubject::Anonymous, object: None, authenticated: true, task: true });
    }
    let user = caller.as_ref().and_then(GitCaller::user);
    let mut ctx = match state.services.repositories.open_following_renames(user, owner, repo).await {
        Ok(ctx) => ctx,
        Err(ServiceError::NotFound) if user.is_none() => return Err(LfsError::Challenge),
        Err(error) => return Err(error.into()),
    };
    if let Some(scope) = caller.as_ref().and_then(GitCaller::token_scope) {
        ctx.access = scope.limit(ctx.access);
    }
    let subject = user.map_or(TokenSubject::Anonymous, |user| TokenSubject::User(user.id()));
    let authenticated = user.is_some();
    Ok(Authorized { ctx, user: caller.and_then(GitCaller::into_user), subject, object: None, authenticated, task: false })
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")).map(str::trim)
}

fn parse_oid(oid: &str) -> Result<LfsOid, LfsError> {
    LfsOid::parse(oid).map_err(|error| LfsError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))
}

/// `{public_url}/{owner}/{repo}.git/info/lfs`, under the repository's current name.
pub(crate) fn endpoint(state: &AppState, ctx: &RepoContext) -> String {
    format!("{}/{}.git/info/lfs", state.config.server.base_url(), ctx.repo.full_name())
}

// ------------------------------------------------------------------ batch

#[derive(Debug, Deserialize)]
struct BatchRequest {
    operation: String,
    #[serde(default)]
    transfers: Vec<String>,
    #[serde(default)]
    objects: Vec<BatchRequestEntry>,
    hash_algo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BatchRequestEntry {
    #[serde(default)]
    oid: String,
    #[serde(default)]
    size: i64,
}

/// `POST /{owner}/{repo}/info/lfs/objects/batch`
async fn batch(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_repository_token()?;
    let request: BatchRequest = parse_json(&body)?;
    let operation = LfsOperation::parse(&request.operation)
        .ok_or_else(|| LfsError::new(StatusCode::UNPROCESSABLE_ENTITY, "the operation must be upload or download"))?;
    if !request.transfers.is_empty() && !request.transfers.iter().any(|t| t == "basic") {
        return Err(LfsError::new(StatusCode::UNPROCESSABLE_ENTITY, "only the basic transfer adapter is supported"));
    }
    if request.hash_algo.as_deref().is_some_and(|algo| algo != "sha256") {
        return Err(LfsError::new(StatusCode::CONFLICT, "only sha256 object ids are supported"));
    }
    if operation.is_write() {
        authorized.require_write(&state, true).await?;
    }

    let objects: Vec<BatchRequestObject> = request.objects.into_iter().map(|o| BatchRequestObject { oid: o.oid, size: o.size }).collect();
    let answers = state.services.lfs.batch(&authorized.ctx, authorized.user.as_ref(), authorized.subject, operation, &objects).await?;
    let objects_url = format!("{}/objects", endpoint(&state, &authorized.ctx));
    // Actions: S. A task's transfers carry its own credential, checked again on every one.
    let task_header = authorized.task.then(|| headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok())).flatten();
    let action = |href: &str, token: &IssuedToken| match task_header {
        Some(header) => json!({ "href": href, "header": { "Authorization": header } }),
        None => action(href, token),
    };
    let objects: Vec<Value> = answers
        .into_iter()
        .map(|answer| {
            let href = format!("{objects_url}/{}", answer.oid);
            let mut object = json!({ "oid": answer.oid, "size": answer.size, "authenticated": true });
            match answer.outcome {
                BatchOutcome::Download(token) => object["actions"] = json!({ "download": action(&href, &token) }),
                BatchOutcome::Upload(token) => {
                    object["actions"] = json!({ "upload": action(&href, &token), "verify": action(&format!("{href}/verify"), &token) });
                }
                BatchOutcome::Present => {}
                BatchOutcome::Missing => object["error"] = json!({ "code": 404, "message": "Object does not exist" }),
                BatchOutcome::Invalid(message) => object["error"] = json!({ "code": 422, "message": message }),
                BatchOutcome::TooLarge(message) => object["error"] = json!({ "code": 413, "message": message }),
            }
            object
        })
        .collect();
    Ok(json_response(StatusCode::OK, &json!({ "transfer": "basic", "objects": objects, "hash_algo": "sha256" })))
}

fn action(href: &str, token: &IssuedToken) -> Value {
    json!({ "href": href, "header": { "Authorization": token.authorization }, "expires_in": token.expires_in })
}

// ------------------------------------------------------------------ basic transfers

/// `GET /{owner}/{repo}/info/lfs/objects/{oid}`: the object's bytes, streamed.
async fn download(
    State(state): State<AppState>,
    Path((owner, repo, oid)): Path<(String, String, String)>,
    client: Client,
    headers: HeaderMap,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    let oid = parse_oid(&oid)?;
    authorized.require_object(&oid)?;
    let download = state.services.lfs.open(&authorized.ctx, authorized.user.as_ref(), &oid).await?;
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
        (header::CONTENT_LENGTH, HeaderValue::from(download.size)),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache")),
    ];
    Ok((headers, Body::from_stream(ReaderStream::new(download.file))).into_response())
}

/// `PUT /{owner}/{repo}/info/lfs/objects/{oid}`: the body is the object. Its size is
/// the one the transfer token was issued for, or else `Content-Length`.
async fn upload(
    State(state): State<AppState>,
    Path((owner, repo, oid)): Path<(String, String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_write(&state, true).await?;
    let oid = parse_oid(&oid)?;
    authorized.require_object(&oid)?;
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let Some(size) = authorized.object.as_ref().map(|(_, size)| *size).or(declared) else {
        return Err(LfsError::new(StatusCode::LENGTH_REQUIRED, "Content-Length is required"));
    };
    let max = state.services.lfs.max_object_bytes();
    if size > max {
        let message = format!("objects can be at most {}", rbit_services::releases::human_bytes(max));
        return Err(LfsError::new(StatusCode::PAYLOAD_TOO_LARGE, message));
    }
    let body = StreamReader::new(body.into_data_stream().map_err(std::io::Error::other));
    state.services.lfs.store(&authorized.ctx, &oid, size, body).await?;
    Ok(StatusCode::OK.into_response())
}

#[derive(Debug, Deserialize)]
struct VerifyRequest {
    oid: String,
    size: u64,
}

/// `POST /{owner}/{repo}/info/lfs/objects/{oid}/verify`: `200` once the upload is stored.
async fn verify(
    State(state): State<AppState>,
    Path((owner, repo, oid)): Path<(String, String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, LfsError> {
    let authorized = authorize(&state, &client, &headers, &owner, &repo).await?;
    authorized.require_write(&state, false).await?;
    let oid = parse_oid(&oid)?;
    authorized.require_object(&oid)?;
    let request: VerifyRequest = parse_json(&body)?;
    if request.oid != oid.as_str() {
        return Err(LfsError::new(StatusCode::UNPROCESSABLE_ENTITY, "the object id does not match the URL"));
    }
    state.services.lfs.verify(&authorized.ctx, &oid, request.size).await?;
    Ok(json_response(StatusCode::OK, &json!({ "message": "ok" })))
}

#[cfg(test)]
mod tests;
