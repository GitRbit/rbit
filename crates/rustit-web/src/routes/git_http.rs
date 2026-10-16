//! Git smart-HTTP protocol endpoints.
//!
//! Authentication is HTTP Basic: username or email + password, or any username + a
//! personal access token (`rit_…`) whose scope caps what the request may do. Cookies are
//! ignored here.
//! Suspended users are refused whichever credential they present.
//! Anonymous users may fetch public repositories. When access is missing the server
//! answers `401` with a `WWW-Authenticate` challenge so Git prompts for credentials.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use async_compression::tokio::bufread::GzipDecoder;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::TryStreamExt;
use rustit_auth::AccessTokenSecret;
use rustit_core::{TokenScope, User};
use rustit_services::{RepoContext, ServiceError};
use rustit_transport::Service;
use serde::Deserialize;
use tokio::io::{AsyncRead, BufReader};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct InfoRefsQuery {
    service: Option<String>,
}

/// `GET /{owner}/{repo}.git/info/refs?service=git-upload-pack|git-receive-pack`
pub async fn info_refs(State(state): State<AppState>, Path((owner, repo)): Path<(String, String)>, Query(query): Query<InfoRefsQuery>, headers: HeaderMap) -> Response {
    let Some(service) = query.service.as_deref().and_then(Service::from_wire_name) else {
        return (StatusCode::FORBIDDEN, "the dumb HTTP protocol is not supported; please upgrade git").into_response();
    };
    let ctx = match authorize(&state, &headers, &owner, &repo, service).await {
        Ok(ctx) => ctx,
        Err(denied) => return denied.into_response(),
    };

    match state.transport.advertise_refs(service, &ctx.disk_path, git_protocol(&headers)).await {
        Ok(body) => (
            [(header::CONTENT_TYPE, service.advertisement_content_type()), (header::CACHE_CONTROL, "no-cache".to_owned())],
            body,
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, %owner, %repo, "ref advertisement failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /{owner}/{repo}.git/git-upload-pack` (fetch, clone)
pub async fn upload_pack(state: State<AppState>, path: Path<(String, String)>, headers: HeaderMap, body: Body) -> Response {
    run_service(state, path, headers, body, Service::UploadPack).await
}

/// `POST /{owner}/{repo}.git/git-receive-pack` (push)
pub async fn receive_pack(state: State<AppState>, path: Path<(String, String)>, headers: HeaderMap, body: Body) -> Response {
    run_service(state, path, headers, body, Service::ReceivePack).await
}

async fn run_service(State(state): State<AppState>, Path((owner, repo)): Path<(String, String)>, headers: HeaderMap, body: Body, service: Service) -> Response {
    let ctx = match authorize(&state, &headers, &owner, &repo, service).await {
        Ok(ctx) => ctx,
        Err(denied) => return denied.into_response(),
    };

    let request = request_reader(&headers, body);
    let session = match state.transport.serve(service, &ctx.disk_path, git_protocol(&headers), request) {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(%error, %owner, %repo, %service, "failed to start git");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if service.is_write() {
        let repositories = state.services.repositories.clone();
        let repo_id = ctx.repo.repo.id;
        tokio::spawn(async move {
            if session_succeeded(session.completion).await
                && let Err(error) = repositories.record_push(repo_id).await
            {
                tracing::warn!(%error, "failed to record push");
            }
        });
    }

    let mut response = Body::from_stream(ReaderStream::new(session.output)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&service.result_content_type()).expect("static content type"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

async fn session_succeeded(completion: tokio::sync::oneshot::Receiver<std::process::ExitStatus>) -> bool {
    completion.await.is_ok_and(|status| status.success())
}

/// Streams the request body, transparently decompressing `Content-Encoding: gzip`
/// (which Git uses for large negotiation requests).
fn request_reader(headers: &HeaderMap, body: Body) -> Box<dyn AsyncRead + Send + Unpin> {
    let stream = body.into_data_stream().map_err(std::io::Error::other);
    let reader = StreamReader::new(stream);
    let gzipped = headers.get(header::CONTENT_ENCODING).is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"gzip"));
    if gzipped { Box::new(GzipDecoder::new(BufReader::new(reader))) } else { Box::new(reader) }
}

fn git_protocol(headers: &HeaderMap) -> Option<&str> {
    headers.get("git-protocol").and_then(|v| v.to_str().ok())
}

/// Why a Git request was refused.
#[derive(Debug)]
enum Denied {
    /// Ask the client for (better) credentials.
    Challenge,
    Forbidden,
    /// Correct credentials of a suspended user.
    Suspended,
    NotFound,
    Internal,
}

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        match self {
            Self::Challenge => {
                (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"Rustit\", charset=\"UTF-8\"")], "authentication required").into_response()
            }
            Self::Forbidden => (StatusCode::FORBIDDEN, "you do not have permission to push to this repository").into_response(),
            Self::Suspended => (StatusCode::FORBIDDEN, "this account has been suspended").into_response(),
            Self::NotFound => (StatusCode::NOT_FOUND, "repository not found").into_response(),
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

/// Who is making a Git request, and how far their credential reaches.
#[derive(Debug)]
struct GitCaller {
    user: User,
    /// `Some` when authenticated with a personal access token, whose scope caps access.
    /// Password authentication carries the user's full access.
    token_scope: Option<TokenScope>,
}

/// Resolves credentials and checks that they grant `service` on the repository.
async fn authorize(state: &AppState, headers: &HeaderMap, owner: &str, repo: &str, service: Service) -> Result<RepoContext, Denied> {
    let caller = authenticate(state, headers).await?;
    let user = caller.as_ref().map(|caller| &caller.user);

    let mut ctx = match state.services.repositories.open(user, owner, repo).await {
        Ok(ctx) => ctx,
        // Anonymous visitors get a credential prompt instead of a definitive answer.
        Err(ServiceError::NotFound) if user.is_none() => return Err(Denied::Challenge),
        Err(ServiceError::NotFound) => return Err(Denied::NotFound),
        Err(error) => {
            tracing::error!(%error, "git authorization failed");
            return Err(Denied::Internal);
        }
    };

    if let Some(scope) = caller.as_ref().and_then(|caller| caller.token_scope) {
        ctx.access = scope.limit(ctx.access);
    }
    if !service.is_write() || ctx.access.can_write() {
        Ok(ctx)
    } else if caller.is_none() {
        Err(Denied::Challenge)
    } else {
        Err(Denied::Forbidden)
    }
}

/// `Ok(None)` for anonymous requests; a challenge for wrong credentials.
/// A password that looks like a personal access token is checked as one, never as a password.
async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Option<GitCaller>, Denied> {
    let Some((login, password)) = basic_credentials(headers) else {
        return Ok(None);
    };
    if AccessTokenSecret::looks_like_token(&password) {
        // The token identifies its owner on its own; Git's username is ignored.
        return match state.services.tokens.authenticate(&password).await {
            Ok(Some((user, scope))) => Ok(Some(GitCaller { user, token_scope: Some(scope) })),
            Ok(None) => Err(Denied::Challenge),
            Err(error) => {
                tracing::error!(%error, "git token authentication failed");
                Err(Denied::Internal)
            }
        };
    }
    match state.services.accounts.authenticate(&login, password).await {
        Ok(user) => Ok(Some(GitCaller { user, token_scope: None })),
        Err(ServiceError::InvalidCredentials) => Err(Denied::Challenge),
        Err(ServiceError::Forbidden) => Err(Denied::Suspended),
        Err(error) => {
            tracing::error!(%error, "git authentication failed");
            Err(Denied::Internal)
        }
    }
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ").or_else(|| value.strip_prefix("basic "))?;
    let decoded = String::from_utf8(STANDARD.decode(encoded.trim()).ok()?).ok()?;
    let (login, password) = decoded.split_once(':')?;
    Some((login.to_owned(), password.to_owned()))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic YWxpY2U6czNjcjN0OndpdGg6Y29sb25z"));
        assert_eq!(basic_credentials(&headers), Some(("alice".into(), "s3cr3t:with:colons".into())));
    }
}
