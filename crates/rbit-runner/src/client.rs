//! The protocol v1 client (§12.6 of `docs/design/actions.md`).
//!
//! Every call carries `Rbit-Runner-Protocol: 1` and `User-Agent: rbit-runner/<version>`, and
//! runner calls the bearer credential. Redirects are not followed: a runner pointed at the
//! wrong URL says so instead of resending its credential elsewhere. Answers are read
//! within a cap, error bodies become a [`CallError`] whose [`CallError::disposition`] tells
//! the caller whether to retry, give the task up, or stop.
//!
//! Retrying is the caller's loop, paced by a [`Backoff`] (1 s doubling to 60 s, with
//! jitter) or the server's `Retry-After`. A poll carries an [`idempotency_key`] that the
//! caller keeps across retries of the same poll, so a task whose answer was lost comes
//! back on the retry instead of waiting for the sweeper.

use std::{fmt::Write as _, sync::Once, time::Duration};

use rbit_workflow::{
    protocol::{
        ArtifactInfo, ArtifactList, ArtifactQuery, BASE_PATH, CONTENT_SHA256_HEADER, DeclareRequest, DeclareResponse, ErrorBody, ErrorCode,
        IDEMPOTENCY_KEY_HEADER, LogsResponse, PROTOCOL_HEADER, PROTOCOL_VERSION, PollRequest, RegisterRequest, RegisterResponse, TaskId,
        TaskStateRequest, TaskStateResponse, USER_AGENT_PREFIX, UnregisterRequest,
    },
    secret::Secret,
    spec::JobSpec,
};
use reqwest::{Method, RequestBuilder, Response, StatusCode, Url, header};
use serde::de::DeserializeOwned;

/// How long a call other than a poll may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Artifact uploads and downloads (the server allows them an hour).
const ARTIFACT_TIMEOUT: Duration = Duration::from_hours(1);
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

/// What to do about a failed call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The server is unreachable, busy or failing: try again after a backoff (or the
    /// server's `Retry-After`).
    Retry,
    /// The task or runner no longer exists on the server (`404`).
    Gone,
    /// Retrying cannot help: the credential was refused or the protocol is not served.
    Fatal,
    /// The server refused this one call (`400`, `403`, `409`, `413`); a different call may
    /// succeed.
    Refused,
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

impl CallError {
    pub fn disposition(&self) -> Disposition {
        match self {
            Self::Unreachable(_) => Disposition::Retry,
            Self::Malformed(_) => Disposition::Refused,
            Self::Status { status, body, .. } => match body.code {
                ErrorCode::NotFound => Disposition::Gone,
                ErrorCode::Unauthenticated | ErrorCode::UnsupportedProtocol => Disposition::Fatal,
                ErrorCode::BadRequest | ErrorCode::Forbidden | ErrorCode::Conflict | ErrorCode::TooLarge => Disposition::Refused,
                // A code from a newer server: its status class decides.
                ErrorCode::Unknown if (400..500).contains(status) && *status != 408 => Disposition::Refused,
                ErrorCode::Throttled | ErrorCode::Unavailable | ErrorCode::Unknown => Disposition::Retry,
            },
        }
    }

    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Status { body, .. } => Some(body.code),
            _ => None,
        }
    }

    /// The server's `Retry-After`, when it sent one.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Status { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// The line to resend logs from, on a `409` log gap.
    pub fn next_line(&self) -> Option<u64> {
        match self {
            Self::Status { body, .. } if body.code == ErrorCode::Conflict => body.next_line,
            _ => None,
        }
    }
}

/// Exponential backoff with jitter: each delay is between half and all of a ceiling that
/// starts at `first` and doubles up to `max`. Jitter keeps runners that lost the server at
/// the same moment from coming back in lockstep.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    first: Duration,
    max: Duration,
    ceiling: Duration,
}

impl Backoff {
    /// The reconnect backoff of §12.5.4: 1 s to 60 s.
    pub const RECONNECT: Self = Self::new(Duration::from_secs(1), Duration::from_secs(60));

    pub const fn new(first: Duration, max: Duration) -> Self {
        Self { first, max, ceiling: first }
    }

    /// The next delay; the ceiling then doubles.
    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self.ceiling;
        self.ceiling = (ceiling * 2).min(self.max);
        let half = ceiling / 2;
        let random = getrandom::u64().unwrap_or(u64::MAX / 2);
        // `random / u64::MAX` of the other half, in nanoseconds; a ceiling fits u64 nanoseconds.
        let spread = u64::try_from(half.as_nanos()).unwrap_or(u64::MAX);
        let jitter = u64::try_from(u128::from(spread) * u128::from(random) / u128::from(u64::MAX)).unwrap_or(spread);
        half + Duration::from_nanos(jitter)
    }

    /// The delay after `error`: the server's `Retry-After` when it sent one.
    pub fn after(&mut self, error: &CallError) -> Duration {
        error.retry_after().unwrap_or_else(|| self.next_delay())
    }

    /// Back to the first delay, after a call succeeded.
    pub fn reset(&mut self) {
        self.ceiling = self.first;
    }
}

/// A fresh [`IDEMPOTENCY_KEY_HEADER`] value: 128 random bits in hex.
pub fn idempotency_key() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random number generator is unavailable");
    bytes.iter().fold(String::with_capacity(32), |mut key, byte| {
        let _ = write!(key, "{byte:02x}");
        key
    })
}

/// What a poll brought.
#[derive(Debug)]
pub enum Claimed {
    Task(Box<JobSpec>),
    /// A task this runner cannot read, with its id when that much could be read, so it
    /// can still be failed instead of left to the sweeper.
    Unreadable {
        task_id: Option<TaskId>,
        error: String,
    },
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
        let response: RegisterResponse = self.json(self.request(Method::POST, "register").json(request)).await?;
        response.validate().map_err(|error| CallError::Malformed(error.message))?;
        Ok(response)
    }

    /// `POST declare`, at every start.
    pub async fn declare(&self, request: &DeclareRequest) -> Result<DeclareResponse, CallError> {
        let response: DeclareResponse = self.json(self.request(Method::POST, "declare").json(request)).await?;
        response.intervals.validate().map_err(|error| CallError::Malformed(error.message))?;
        Ok(response)
    }

    /// `POST poll`: a task, or `None` when the poll ended empty. The server holds the call
    /// for up to `long_poll`, so the timeout allows for that.
    pub async fn poll(&self, free_slots: u32, key: &str, long_poll: Duration) -> Result<Option<Claimed>, CallError> {
        let request = self
            .request(Method::POST, "poll")
            .header(IDEMPOTENCY_KEY_HEADER, key)
            .timeout(long_poll + CALL_TIMEOUT)
            .json(&PollRequest { free_slots });
        let response = self.send(request).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let bytes = read_capped(response, MAX_ANSWER_BODY).await?;
        Ok(Some(claimed(&bytes)))
    }

    /// `POST tasks/{id}/state`.
    pub async fn task_state(&self, task: TaskId, request: &TaskStateRequest) -> Result<TaskStateResponse, CallError> {
        self.json(self.request(Method::POST, &format!("tasks/{task}/state")).json(request)).await
    }

    /// `POST tasks/{id}/logs?from=<line>`: `body` is whole lines, each ending in `\n`.
    pub async fn task_logs(&self, task: TaskId, from: u64, body: Vec<u8>) -> Result<LogsResponse, CallError> {
        let request = self
            .request(Method::POST, &format!("tasks/{task}/logs?from={from}"))
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(body);
        self.json(request).await
    }

    // Actions: artifacts (A).
    /// `PUT tasks/{id}/artifacts/{name}`: uploads the tar.gz at `archive`, whose SHA-256 is
    /// `sha256` (hex). Artifacts may be large, so the call may take up to an hour.
    pub async fn upload_artifact(
        &self,
        task: TaskId,
        name: &str,
        query: &ArtifactQuery,
        sha256: &str,
        archive: &std::path::Path,
    ) -> Result<ArtifactInfo, CallError> {
        let file =
            tokio::fs::File::open(archive).await.map_err(|error| CallError::Unreachable(format!("cannot read the archive: {error}")))?;
        let length = file.metadata().await.map(|metadata| metadata.len()).unwrap_or_default();
        let path = match query.retention_days {
            Some(days) => format!("tasks/{task}/artifacts/{name}?overwrite={}&retention_days={days}", query.overwrite),
            None => format!("tasks/{task}/artifacts/{name}?overwrite={}", query.overwrite),
        };
        let request = self
            .request(Method::PUT, &path)
            .header(CONTENT_SHA256_HEADER, sha256)
            .header(header::CONTENT_TYPE, "application/gzip")
            .header(header::CONTENT_LENGTH, length)
            .timeout(ARTIFACT_TIMEOUT)
            .body(reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file)));
        self.json(request).await
    }

    /// `GET tasks/{id}/artifacts`: the run's artifacts so far.
    pub async fn artifacts(&self, task: TaskId, run: Option<i64>) -> Result<ArtifactList, CallError> {
        self.json(self.request(Method::GET, &format!("tasks/{task}/artifacts{}", run_query(run)))).await
    }

    /// `GET tasks/{id}/artifacts/{name}` into a new file at `out`; the SHA-256 (hex) the
    /// server sent with it.
    pub async fn download_artifact(&self, task: TaskId, run: Option<i64>, name: &str, out: &std::path::Path) -> Result<String, CallError> {
        let request = self.request(Method::GET, &format!("tasks/{task}/artifacts/{name}{}", run_query(run))).timeout(ARTIFACT_TIMEOUT);
        let mut response = self.send(request).await?;
        let digest = response.headers().get(CONTENT_SHA256_HEADER).and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        let local = |error: std::io::Error| CallError::Unreachable(format!("cannot write the artifact: {error}"));
        let mut file = tokio::fs::File::create(out).await.map_err(local)?;
        while let Some(chunk) = response.chunk().await.map_err(|error| CallError::Unreachable(transport_message(&error)))? {
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.map_err(local)?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(local)?;
        Ok(digest)
    }

    /// `POST unregister`.
    pub async fn unregister(&self) -> Result<(), CallError> {
        self.send(self.request(Method::POST, "unregister").json(&UnregisterRequest {})).await.map(drop)
    }
}

/// Reads a poll's `{task: JobSpec}`, validated. When it does not parse, the task id is
/// looked for on its own.
fn claimed(bytes: &[u8]) -> Claimed {
    match serde_json::from_slice::<rbit_workflow::protocol::PollResponse>(bytes) {
        Ok(response) => match response.task.validate() {
            Ok(()) => Claimed::Task(Box::new(response.task)),
            Err(error) => Claimed::Unreadable { task_id: Some(response.task.task_id), error: error.message },
        },
        Err(error) => {
            let task_id = serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .and_then(|value| value.pointer("/task/task_id").and_then(serde_json::Value::as_i64))
                .map(TaskId);
            Claimed::Unreadable { task_id, error: error.to_string() }
        }
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

// Actions: workflow_run.
/// `?run_id=<run>` for another run's artifacts, or nothing for the task's own.
fn run_query(run: Option<i64>) -> String {
    run.map(|run| format!("?run_id={run}")).unwrap_or_default()
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

    fn status(status: u16, code: ErrorCode) -> CallError {
        CallError::Status { status, body: ErrorBody::new(code, "m"), retry_after: None }
    }

    #[test]
    fn errors_are_sorted_by_what_to_do_next() {
        let cases = [
            (status(404, ErrorCode::NotFound), Disposition::Gone),
            (status(401, ErrorCode::Unauthenticated), Disposition::Fatal),
            (status(426, ErrorCode::UnsupportedProtocol), Disposition::Fatal),
            (status(400, ErrorCode::BadRequest), Disposition::Refused),
            (status(403, ErrorCode::Forbidden), Disposition::Refused),
            (status(409, ErrorCode::Conflict), Disposition::Refused),
            (status(413, ErrorCode::TooLarge), Disposition::Refused),
            (status(429, ErrorCode::Throttled), Disposition::Retry),
            (status(503, ErrorCode::Unavailable), Disposition::Retry),
            (status(502, ErrorCode::Unknown), Disposition::Retry),
            (status(408, ErrorCode::Unknown), Disposition::Retry),
            (status(418, ErrorCode::Unknown), Disposition::Refused),
            (CallError::Unreachable("refused".into()), Disposition::Retry),
            (CallError::Malformed("x".into()), Disposition::Refused),
        ];
        for (error, expected) in cases {
            assert_eq!(error.disposition(), expected, "{error}");
        }
        let gap = CallError::Status { status: 409, body: ErrorBody::log_gap(7), retry_after: None };
        assert_eq!(gap.next_line(), Some(7));
        assert_eq!(status(409, ErrorCode::Conflict).next_line(), None);
    }

    #[test]
    fn backoff_doubles_to_its_cap_with_jitter() {
        let mut backoff = Backoff::RECONNECT;
        let ceilings = [1, 2, 4, 8, 16, 32, 60, 60, 60];
        for ceiling in ceilings {
            let ceiling = Duration::from_secs(ceiling);
            let delay = backoff.next_delay();
            assert!(delay >= ceiling / 2 && delay <= ceiling, "{delay:?} outside {ceiling:?}");
        }
        backoff.reset();
        assert!(backoff.next_delay() <= Duration::from_secs(1));
        let throttled =
            CallError::Status { status: 429, body: ErrorBody::new(ErrorCode::Throttled, "m"), retry_after: Some(Duration::from_secs(42)) };
        assert_eq!(backoff.after(&throttled), Duration::from_secs(42));
    }

    #[test]
    fn idempotency_keys_are_fresh_and_well_formed() {
        let (first, second) = (idempotency_key(), idempotency_key());
        assert_ne!(first, second);
        assert!(rbit_workflow::protocol::valid_idempotency_key(&first) && first.len() == 32, "{first}");
    }

    #[test]
    fn unreadable_tasks_keep_their_id() {
        match claimed(br#"{"task":{"task_id":12,"version":99}}"#) {
            Claimed::Unreadable { task_id, error } => assert_eq!((task_id, error.is_empty()), (Some(TaskId(12)), false)),
            Claimed::Task(_) => panic!("an invalid spec was accepted"),
        }
        assert!(matches!(claimed(b"not json"), Claimed::Unreadable { task_id: None, .. }));
        let spec = crate::testing::job_spec(5, 1);
        let body = serde_json::to_vec(&rbit_workflow::protocol::PollResponse { task: spec.clone() }).unwrap();
        assert!(matches!(claimed(&body), Claimed::Task(task) if *task == spec));
        let mut invalid = spec;
        invalid.token = Secret::new("rbat_short");
        let body = serde_json::to_vec(&rbit_workflow::protocol::PollResponse { task: invalid }).unwrap();
        assert!(matches!(claimed(&body), Claimed::Unreadable { task_id: Some(TaskId(5)), .. }));
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
