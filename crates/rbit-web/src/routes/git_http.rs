//! Git smart-HTTP protocol endpoints.
//!
//! Authentication is HTTP Basic: username or email + password, or any username + a
//! personal access token (`rbit_…`, or the legacy `rit_…`) whose scope caps what the request
//! may do. Cookies are ignored here.
//! Suspended users are refused whichever credential they present.
//! Anonymous users may fetch public repositories. When access is missing the server
//! answers `401` with a `WWW-Authenticate` challenge so Git prompts for credentials.
//!
//! Credentials are throttled per client address and per login (`429` + `Retry-After`).
//! Accounts with two-factor authentication must use a personal access token: their
//! password is refused with an explanation Git prints (`fatal: remote error: …`).
//! A repository's old `owner/name` from before a rename or transfer is served as the
//! repository itself, so remotes configured with it keep working.
//! Pushes to pull mirrors are refused (`403`): a mirror only changes by fetching its
//! upstream. Every other push passes the push gate ([`push_gate`]: branch protection
//! reads the command list before git starts) after those checks.

use async_compression::tokio::bufread::GzipDecoder;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::TryStreamExt;
use rbit_auth::AccessTokenSecret;
use rbit_core::{TokenScope, User};
use rbit_services::{RepoContext, ServiceError, ThrottleScope, mirrors, protection::PushActor, two_factor::GitPasswordOutcome};
use rbit_transport::{
    ReceiveOptions, Service,
    receive::{self, CommandLimits, Intercept},
};
use serde::Deserialize;
use tokio::io::{AsyncRead, BufReader};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::{AppState, client_ip::Client};

#[derive(Debug, Deserialize)]
pub struct InfoRefsQuery {
    service: Option<String>,
}

/// `GET /{owner}/{repo}.git/info/refs?service=git-upload-pack|git-receive-pack`
pub async fn info_refs(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<InfoRefsQuery>,
    client: Client,
    headers: HeaderMap,
) -> Response {
    let Some(service) = query.service.as_deref().and_then(Service::from_wire_name) else {
        return (StatusCode::FORBIDDEN, "the dumb HTTP protocol is not supported; please upgrade git").into_response();
    };
    let ctx = match authorize(&state, &client, &headers, &owner, &repo, service).await {
        Ok((ctx, _)) => ctx,
        // The advertisement is the first request of every operation, so the message reaches the user.
        Err(Denied::TokenRequired) => return remote_error(service, TOKEN_REQUIRED),
        Err(denied) => return denied.into_response(),
    };

    match state.transport.advertise_refs(service, &ctx.disk_path, git_protocol(&headers)).await {
        Ok(body) => ([(header::CONTENT_TYPE, service.advertisement_content_type()), (header::CACHE_CONTROL, "no-cache".to_owned())], body)
            .into_response(),
        Err(error) => {
            tracing::error!(%error, %owner, %repo, "ref advertisement failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /{owner}/{repo}.git/git-upload-pack` (fetch, clone)
pub async fn upload_pack(state: State<AppState>, path: Path<(String, String)>, client: Client, headers: HeaderMap, body: Body) -> Response {
    run_service(state, path, client, headers, body, Service::UploadPack).await
}

/// `POST /{owner}/{repo}.git/git-receive-pack` (push)
pub async fn receive_pack(
    state: State<AppState>,
    path: Path<(String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Body,
) -> Response {
    run_service(state, path, client, headers, body, Service::ReceivePack).await
}

async fn run_service(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    client: Client,
    headers: HeaderMap,
    body: Body,
    service: Service,
) -> Response {
    let (ctx, user) = match authorize(&state, &client, &headers, &owner, &repo, service).await {
        Ok(authorized) => authorized,
        Err(denied) => return denied.into_response(),
    };

    let request = request_reader(&headers, body);
    // The push gate runs on the resolved repository, after the access and mirror checks.
    let (request, options) = if service.is_write() {
        match push_gate(&state, &ctx, user.as_ref(), request).await {
            Gate::Pass(request, options) => (request, options),
            Gate::Answer(response) => return response,
        }
    } else {
        (request, ReceiveOptions::default())
    };

    // Taken after the verdict and before git starts, so the push's ref updates can be
    // reported afterwards.
    let pending = if service.is_write() { Some(state.services.pushes.begin(&ctx, user.as_ref().map(User::id)).await) } else { None };
    let session = match state.transport.serve(service, &ctx.disk_path, git_protocol(&headers), options, request) {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(%error, %owner, %repo, %service, "failed to start git");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Some(pending) = pending {
        let recorder = state.services.pushes.clone();
        tokio::spawn(async move {
            if session_succeeded(session.completion).await {
                recorder.complete(pending).await;
            }
        });
    }

    let mut response = Body::from_stream(ReaderStream::new(session.output)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&service.result_content_type()).expect("static content type"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

type Request = Box<dyn AsyncRead + Send + Unpin>;

/// What the push gate decided about a receive-pack request.
enum Gate {
    /// Run git on this input (the command list replayed, then the rest) with these options.
    Pass(Request, ReceiveOptions),
    /// Answer without starting git: a refusal report, or an error.
    Answer(Response),
}

/// Reads the push's command list before git does and lets branch protection decide
/// (`docs/design/milestone-2b.md` §4). A refused push is answered all-or-nothing, like a
/// declining `pre-receive` hook: with the client's report (`ng <ref> <reason>`) or, for
/// clients that asked for none, `403` and a text explanation.
async fn push_gate(state: &AppState, ctx: &RepoContext, user: Option<&User>, request: Request) -> Gate {
    let (commands, input) = match receive::intercept(request, CommandLimits::default()).await {
        Ok(Intercept::Unread(request)) => return Gate::Pass(request, ReceiveOptions::default()),
        Ok(Intercept::Read { commands, input }) => (commands, input),
        Err(error) => return Gate::Answer((StatusCode::BAD_REQUEST, error.to_string()).into_response()),
    };
    let actor = PushActor { user, deploy_key: false, access: ctx.access };
    let verdict = match state.services.protection.check_push(ctx, &actor, &commands.updates()).await {
        Ok(verdict) => verdict,
        Err(error) => {
            tracing::error!(%error, repo = %ctx.repo.full_name(), "push gate failed");
            return Gate::Answer(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    if !verdict.is_refused() {
        let options = ReceiveOptions { deny_non_fast_forwards: verdict.deny_non_fast_forwards };
        return Gate::Pass(Box::new(input), options);
    }
    receive::drain(input).await;
    let report = receive::refusal(&commands, &verdict.refusals);
    if report.is_empty() {
        return Gate::Answer((StatusCode::FORBIDDEN, receive::refusal_message(&verdict.refusals)).into_response());
    }
    let content_type = Service::ReceivePack.result_content_type();
    Gate::Answer(([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache".to_owned())], report).into_response())
}

async fn session_succeeded(completion: tokio::sync::oneshot::Receiver<std::process::ExitStatus>) -> bool {
    completion.await.is_ok_and(|status| status.success())
}

/// Streams the request body, transparently decompressing `Content-Encoding: gzip`
/// (which Git uses for large negotiation requests).
fn request_reader(headers: &HeaderMap, body: Body) -> Request {
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
    /// The account password of a user with two-factor authentication.
    TokenRequired,
    /// Too many attempts from this client or against this login.
    RateLimited {
        retry_after_seconds: u64,
    },
    /// A push to a pull mirror.
    Mirror,
    NotFound,
    Internal,
}

/// Shown by Git when an account with two-factor authentication offers its password.
const TOKEN_REQUIRED: &str = "two-factor authentication is enabled for this account, so its password cannot be used \
                              with Git. Create a personal access token under Settings > Access tokens and use it as the \
                              password.";

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        match self {
            Self::Challenge => (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"rbit\", charset=\"UTF-8\"")],
                "authentication required",
            )
                .into_response(),
            Self::Forbidden => (StatusCode::FORBIDDEN, "you do not have permission to push to this repository").into_response(),
            Self::Suspended => (StatusCode::FORBIDDEN, "this account has been suspended").into_response(),
            Self::TokenRequired => (StatusCode::FORBIDDEN, TOKEN_REQUIRED).into_response(),
            Self::RateLimited { retry_after_seconds } => (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after_seconds.to_string())],
                format!("too many authentication attempts; try again in {retry_after_seconds} seconds"),
            )
                .into_response(),
            Self::Mirror => (StatusCode::FORBIDDEN, mirrors::PUSH_REFUSED).into_response(),
            Self::NotFound => (StatusCode::NOT_FOUND, "repository not found").into_response(),
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

/// A ref advertisement consisting of a single `ERR` packet: Git aborts and prints
/// `fatal: remote error: <message>`, which plain HTTP error bodies never achieve.
fn remote_error(service: Service, message: &str) -> Response {
    let payload = format!("ERR {message}\n");
    let packet = format!("{:04x}{payload}", payload.len() + 4);
    ([(header::CONTENT_TYPE, service.advertisement_content_type()), (header::CACHE_CONTROL, "no-cache".to_owned())], packet).into_response()
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
/// Returns the repository and the authenticated account, if any.
async fn authorize(
    state: &AppState,
    client: &Client,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    service: Service,
) -> Result<(RepoContext, Option<User>), Denied> {
    let caller = authenticate(state, client, headers).await?;
    let user = caller.as_ref().map(|caller| &caller.user);

    // An old name from before a rename or transfer keeps working, so existing clones do.
    let mut ctx = match state.services.repositories.open_following_renames(user, owner, repo).await {
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
    if !service.is_write() {
        return Ok((ctx, user.cloned()));
    }
    if !ctx.access.can_write() {
        return Err(if caller.is_none() { Denied::Challenge } else { Denied::Forbidden });
    }
    // Checked after access, so only those who could otherwise push learn it is a mirror.
    match state.services.mirrors.is_mirror(ctx.repo.repo.id).await {
        Ok(false) => Ok((ctx, user.cloned())),
        Ok(true) => Err(Denied::Mirror),
        Err(error) => {
            tracing::error!(%error, "git mirror check failed");
            Err(Denied::Internal)
        }
    }
}

/// `Ok(None)` for anonymous requests; a challenge for wrong credentials.
/// A password that looks like a personal access token is checked as one, never as a password.
async fn authenticate(state: &AppState, client: &Client, headers: &HeaderMap) -> Result<Option<GitCaller>, Denied> {
    let Some((login, password)) = basic_credentials(headers) else {
        return Ok(None);
    };
    if AccessTokenSecret::looks_like_token(&password) {
        // Tokens are unguessable, but a client spraying them still spends its budget.
        let ip = client.ip().map(|ip| ip.to_canonical().to_string()).unwrap_or_default();
        if let Err(ServiceError::RateLimited { retry_after_seconds }) = state.services.throttle.check_client(ThrottleScope::GitAuth, &ip) {
            return Err(Denied::RateLimited { retry_after_seconds });
        }
        // The token identifies its owner on its own; Git's username is ignored.
        return match state.services.tokens.authenticate(&password).await {
            Ok(Some((user, scope))) => Ok(Some(GitCaller { user, token_scope: Some(scope) })),
            Ok(None) => {
                state.services.throttle.record_client_failure(ThrottleScope::GitAuth, &ip);
                Err(Denied::Challenge)
            }
            Err(error) => {
                tracing::error!(%error, "git token authentication failed");
                Err(Denied::Internal)
            }
        };
    }
    match state.services.authenticate_git_password(&login, password, &client.0).await {
        Ok(GitPasswordOutcome::Authenticated(user)) => Ok(Some(GitCaller { user, token_scope: None })),
        Ok(GitPasswordOutcome::TokenRequired) => Err(Denied::TokenRequired),
        Err(ServiceError::InvalidCredentials) => Err(Denied::Challenge),
        Err(ServiceError::Forbidden) => Err(Denied::Suspended),
        Err(ServiceError::RateLimited { retry_after_seconds }) => Err(Denied::RateLimited { retry_after_seconds }),
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

    #[tokio::test]
    async fn remote_errors_are_single_err_packets() {
        let response = remote_error(Service::UploadPack, "use a token");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/x-git-upload-pack-advertisement");
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"0014ERR use a token\n");
    }

    #[tokio::test]
    async fn pushes_to_mirrors_are_forbidden_with_an_explanation() {
        let response = Denied::Mirror.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], mirrors::PUSH_REFUSED.as_bytes());
    }

    #[test]
    fn throttled_credentials_carry_retry_after() {
        let response = Denied::RateLimited { retry_after_seconds: 12 }.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "12");
    }
}
