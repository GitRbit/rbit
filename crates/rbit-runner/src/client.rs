//! The protocol v1 client (§12.6 of `docs/design/actions.md`).
//!
//! Every call carries `Rbit-Runner-Protocol: 1` and `User-Agent: rbit-runner/<version>`, and
//! runner calls the bearer credential. Redirects are not followed: a runner pointed at the
//! wrong URL says so instead of resending its credential elsewhere. Answers are read
//! within a cap, error bodies become a [`CallError`] whose [`CallError::disposition`] tells
//! the caller whether to retry, give the task up, or stop.

use std::{sync::Once, time::Duration};

use rbit_workflow::{
    protocol::{BASE_PATH, ErrorBody, ErrorCode, PROTOCOL_HEADER, PROTOCOL_VERSION, RegisterRequest, RegisterResponse, USER_AGENT_PREFIX},
    secret::Secret,
};
use reqwest::{RequestBuilder, Response, StatusCode, Url, header};
use serde::de::DeserializeOwned;

/// How long a call other than a poll may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest `Retry-After` honoured; a larger one is clamped.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(600);
/// The largest error body read.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// The largest successful answer read: a job specification may carry up to 1024 secrets
/// of 64 KiB, plus its contexts.
pub const MAX_ANSWER_BODY: usize = 80 * 1024 * 1024;

/// The `User-Agent` of every call.
pub fn user_agent() -> String {
    format!("{USER_AGENT_PREFIX}{}", env!("CARGO_PKG_VERSION"))
}

static CRYPTO_PROVIDER: Once = Once::new();

/// Makes `ring` the process's rustls provider, as the server does for its webhooks.
pub fn install_crypto_provider() {
    CRYPTO_PROVIDER.call_once(|| {
        // An error only means another provider was installed first, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A connection to one rbit instance.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    /// `<server>/api/actions/runner/v1/`.
    base: Url,
    credential: Option<Secret>,
}

/// A failed call.
#[derive(Debug, Clone)]
pub enum CallError {
    /// No answer: DNS, connect, TLS, a timeout or a broken connection.
    Unreachable(String),
    /// An error answer.
    Status { status: u16, body: ErrorBody, retry_after: Option<Duration> },
    /// A success answer that is not the message it should be.
    Malformed(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(message) => write!(f, "the server is unreachable: {message}"),
            Self::Malformed(message) => write!(f, "the server sent a malformed answer: {message}"),
            Self::Status { body, .. } if body.code == ErrorCode::UnsupportedProtocol => {
                let versions: Vec<_> = body.supported_versions.iter().map(u32::to_string).collect();
                write!(
                    f,
                    "the server does not speak runner protocol {PROTOCOL_VERSION} (it speaks {}); install the rbit-runner \
                     release that matches the server",
                    if versions.is_empty() { "none it named".to_owned() } else { versions.join(", ") }
                )
            }
            Self::Status { status, body, retry_after } => {
                write!(f, "the server answered {status} {}: {}", code_name(body.code), printable(&body.message))?;
                match retry_after {
                    Some(after) => write!(f, " (retry after {}s)", after.as_secs()),
                    None => Ok(()),
                }
            }
        }
    }
}

impl std::error::Error for CallError {}

fn code_name(code: ErrorCode) -> String {
    serde_json::to_value(code).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_else(|| "unknown".to_owned())
}

/// A server message as it may be printed: no control characters, at most 500 characters.
fn printable(message: &str) -> String {
    message.chars().map(|c| if c.is_control() { ' ' } else { c }).take(500).collect()
}

impl Client {
    /// A client for the server at `server` (see [`crate::config::server_url`]); runner
    /// calls need the `credential`.
    pub fn new(server: &Url, credential: Option<Secret>) -> anyhow::Result<Self> {
        install_crypto_provider();
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(CALL_TIMEOUT)
            .pool_max_idle_per_host(4)
            .build()?;
        let mut base = server.clone();
        base.set_path(&format!("{}{BASE_PATH}/", server.path().trim_end_matches('/')));
        Ok(Self { http, base, credential })
    }

    pub fn endpoint(&self, path: &str) -> Url {
        // `path` is one of the fixed endpoint paths with a numeric id, never user input.
        self.base.join(path).unwrap_or_else(|_| self.base.clone())
    }

    /// A request with the protocol headers and, when the client has one, the credential.
    pub fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        let request = self.http.request(method, self.endpoint(path)).header(PROTOCOL_HEADER, PROTOCOL_VERSION.to_string());
        match &self.credential {
            Some(credential) => request.bearer_auth(credential.expose()),
            None => request,
        }
    }

    /// Sends `request` and returns the answer when it is a success.
    pub async fn send(&self, request: RequestBuilder) -> Result<Response, CallError> {
        let response = request.send().await.map_err(|error| CallError::Unreachable(transport_message(&error)))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|seconds| Duration::from_secs(seconds).min(MAX_RETRY_AFTER));
        let location = response.headers().get(header::LOCATION).and_then(|value| value.to_str().ok()).map(printable);
        let bytes = read_capped(response, MAX_ERROR_BODY).await.unwrap_or_default();
        let body = serde_json::from_slice::<ErrorBody>(&bytes).unwrap_or_else(|_| {
            let message = match location {
                Some(location) if status.is_redirection() => format!("the server redirects to {location}; register with that URL"),
                _ => format!("no error body ({})", status.canonical_reason().unwrap_or("unexpected status")),
            };
            ErrorBody::new(code_for_status(status), message)
        });
        Err(CallError::Status { status: status.as_u16(), body, retry_after })
    }

    /// Sends `request` and decodes a JSON answer.
    pub async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, CallError> {
        let response = self.send(request).await?;
        let bytes = read_capped(response, MAX_ANSWER_BODY).await?;
        serde_json::from_slice(&bytes).map_err(|error| CallError::Malformed(error.to_string()))
    }

    /// `POST register`.
    pub async fn register(&self, request: &RegisterRequest) -> Result<RegisterResponse, CallError> {
        let response: RegisterResponse = self.json(self.request(reqwest::Method::POST, "register").json(request)).await?;
        response.validate().map_err(|error| CallError::Malformed(error.message))?;
        Ok(response)
    }
}

/// The error code an answer without a (valid) error body stands for.
fn code_for_status(status: StatusCode) -> ErrorCode {
    match status.as_u16() {
        400 => ErrorCode::BadRequest,
        401 => ErrorCode::Unauthenticated,
        403 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        413 => ErrorCode::TooLarge,
        426 => ErrorCode::UnsupportedProtocol,
        429 => ErrorCode::Throttled,
        503 => ErrorCode::Unavailable,
        _ => ErrorCode::Unknown,
    }
}

/// The body of `response`, refusing more than `cap` bytes.
pub async fn read_capped(mut response: Response, cap: usize) -> Result<Vec<u8>, CallError> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| CallError::Unreachable(transport_message(&error)))? {
        if body.len() + chunk.len() > cap {
            return Err(CallError::Malformed(format!("the answer is larger than {cap} bytes")));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// A transport error with its causes, which carry the useful part (`connection refused`,
/// the certificate problem).
fn transport_message(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_print_safely() {
        let unsupported = CallError::Status { status: 426, body: ErrorBody::unsupported_protocol(), retry_after: None };
        assert!(unsupported.to_string().contains("it speaks 1"), "{unsupported}");
        let throttled = CallError::Status {
            status: 429,
            body: ErrorBody::new(ErrorCode::Throttled, "slow down"),
            retry_after: Some(Duration::from_secs(3)),
        };
        assert_eq!(throttled.to_string(), "the server answered 429 throttled: slow down (retry after 3s)");
        assert_eq!(code_for_status(StatusCode::BAD_GATEWAY), ErrorCode::Unknown);
        let hostile = CallError::Status { status: 400, body: ErrorBody::new(ErrorCode::BadRequest, "a\x1b[2Jb"), retry_after: None };
        assert!(!hostile.to_string().contains('\x1b'));
    }

    #[test]
    fn endpoints_live_under_the_base_path() {
        let server = Url::parse("https://example.com/rbit").unwrap();
        let client = Client::new(&server, None).unwrap();
        assert_eq!(client.endpoint("register").as_str(), "https://example.com/rbit/api/actions/runner/v1/register");
        assert_eq!(client.endpoint("tasks/3/state").as_str(), "https://example.com/rbit/api/actions/runner/v1/tasks/3/state");
        let root = Client::new(&Url::parse("https://example.com").unwrap(), None).unwrap();
        assert_eq!(root.endpoint("poll").as_str(), "https://example.com/api/actions/runner/v1/poll");
        assert_eq!(user_agent(), format!("rbit-runner/{}", env!("CARGO_PKG_VERSION")));
    }
}
