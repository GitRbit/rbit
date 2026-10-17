//! The HTTP client an import talks to its source with: authentication, pagination through
//! `Link` headers, rate limits, and streamed downloads.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! * Every connection goes through the webhooks' [`GuardedResolver`], so unless
//!   `[imports] allow_private_addresses` no request (API, release file, LFS object) reaches
//!   an internal address, also after DNS changes; address literals are checked before
//!   sending, and proxy environment variables are ignored.
//! * API requests never follow redirects. Downloads follow at most
//!   [`MAX_DOWNLOAD_REDIRECTS`] (GitHub serves release files from another host), each hop
//!   checked like the first; the HTTP client drops the `Authorization` header when a
//!   redirect leaves the original host.
//! * The token is sent as `Authorization: Bearer` (GitHub) or `token` (Gitea), only to the
//!   source's API host; it is kept in memory for the job's lifetime and never logged.
//! * Rate limits: a `403`/`429` with `Retry-After`, or with `X-RateLimit-Remaining: 0`
//!   and `X-RateLimit-Reset`, is retried after the wait (at most
//!   `rate_limit_max_wait_seconds`; a longer wait fails the import with the time it would
//!   end); a successful response that used up the budget pauses until the reset. Server
//!   errors are retried twice with a short backoff.
//! * Pages are followed through `rel="next"` links, which must stay on the API's origin.

use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use rbit_core::{
    Timestamp,
    import::{ImportSource, SourceKind},
    network::is_internal_address,
};
use reqwest::{
    StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER},
};
use serde::de::DeserializeOwned;
use tokio::{io::AsyncRead, sync::mpsc};
use tokio_util::sync::CancellationToken;

use crate::webhooks::delivery::{GuardedResolver, install_crypto_provider};

/// Sent as `User-Agent` (GitHub requires one).
pub const USER_AGENT: &str = concat!("rbit-Importer/", env!("CARGO_PKG_VERSION"));
/// Items per page requested (GitHub's maximum; Gitea caps it at its own maximum).
pub const PER_PAGE: u32 = 100;
/// Redirects a download may follow.
pub const MAX_DOWNLOAD_REDIRECTS: usize = 5;
/// Attempts per request when the server fails or a rate limit makes us wait.
const MAX_ATTEMPTS: u32 = 5;
/// Longest error body excerpt kept.
const MAX_ERROR_BODY: usize = 300;

/// Why talking to the source failed. Messages are shown on the progress page.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SourceError {
    #[error("the source refused the token (401); check that it is valid")]
    Unauthorized,
    #[error("not found on the source (or the token cannot read it): {0}")]
    NotFound(String),
    #[error("the source's rate limit is used up until {0}; try again later")]
    RateLimited(String),
    #[error("the source answered {status}: {message}")]
    Status { status: u16, message: String },
    #[error("cannot reach the source: {0}")]
    Network(String),
    #[error("the source sent something unexpected: {0}")]
    Invalid(String),
    #[error("the import was stopped")]
    Cancelled,
}

/// One import's view of its source.
#[derive(Debug, Clone)]
pub struct SourceClient {
    api: reqwest::Client,
    downloads: reqwest::Client,
    source: Arc<ImportSource>,
    token: Option<Arc<str>>,
    allow_private: bool,
    max_wait: Duration,
    cancel: CancellationToken,
}

impl SourceClient {
    pub fn new(
        source: ImportSource,
        token: Option<String>,
        allow_private: bool,
        timeout: Duration,
        max_wait: Duration,
        cancel: CancellationToken,
    ) -> Result<Self, SourceError> {
        install_crypto_provider();
        let resolver = Arc::new(GuardedResolver::new(allow_private));
        let build = |policy: reqwest::redirect::Policy, timeout: Duration| {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .redirect(policy)
                .no_proxy()
                .timeout(timeout)
                .connect_timeout(timeout.min(Duration::from_secs(30)))
                .pool_max_idle_per_host(2)
                .dns_resolver(Arc::clone(&resolver) as Arc<dyn reqwest::dns::Resolve>)
                .build()
                .map_err(|e| SourceError::Network(e.to_string()))
        };
        let redirects = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_DOWNLOAD_REDIRECTS {
                return attempt.error("too many redirects");
            }
            let url = attempt.url();
            if !matches!(url.scheme(), "http" | "https") {
                return attempt.error("redirect to an unsupported scheme");
            }
            // Names are checked by the resolver; address literals never reach it.
            if !allow_private && let Some(host) = url.host_str() {
                let literal = host.trim_start_matches('[').trim_end_matches(']');
                if literal.parse::<std::net::IpAddr>().is_ok_and(is_internal_address) {
                    return attempt.error("redirect to an internal address");
                }
            }
            attempt.follow()
        });
        Ok(Self {
            api: build(reqwest::redirect::Policy::none(), timeout)?,
            downloads: build(redirects, timeout.saturating_mul(10))?,
            source: Arc::new(source),
            token: token.filter(|t| !t.is_empty()).map(Arc::from),
            allow_private,
            max_wait,
            cancel,
        })
    }

    pub fn source(&self) -> &ImportSource {
        &self.source
    }

    /// The token, for Git and LFS (which use Basic authentication).
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// The token as the API expects it.
    fn api_authorization(&self) -> Option<HeaderValue> {
        let token = self.token.as_deref()?;
        let scheme = if self.source.kind == SourceKind::Gitea { "token" } else { "Bearer" };
        let mut value = HeaderValue::from_str(&format!("{scheme} {token}")).ok()?;
        value.set_sensitive(true);
        Some(value)
    }

    /// Whether `url` is on the API's own origin (the token may go there).
    fn is_api_url(&self, url: &str) -> bool {
        url.starts_with(&format!("{}/", self.source.api_base)) || url == self.source.api_base
    }

    /// Refuses address literals that are internal (names are checked when connecting).
    fn check_literal(&self, url: &reqwest::Url) -> Result<(), SourceError> {
        if self.allow_private {
            return Ok(());
        }
        let host = url.host_str().unwrap_or_default();
        let literal = host.trim_start_matches('[').trim_end_matches(']');
        if literal.parse::<std::net::IpAddr>().is_ok_and(is_internal_address) {
            return Err(SourceError::Network(format!("{host} is not a public address")));
        }
        Ok(())
    }

    /// Sends `request` (rebuilt for every attempt), waiting out rate limits and retrying
    /// server errors. Non-success statuses become errors.
    async fn send(&self, client: &reqwest::Client, build: impl Fn() -> reqwest::RequestBuilder) -> Result<reqwest::Response, SourceError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let request = build().build().map_err(|e| SourceError::Invalid(e.to_string()))?;
            self.check_literal(request.url())?;
            let what = request.url().path().to_owned();
            let response = tokio::select! {
                () = self.cancel.cancelled() => return Err(SourceError::Cancelled),
                response = client.execute(request) => response,
            };
            let response = match response {
                Ok(response) => response,
                Err(error) if attempt < 3 && (error.is_timeout() || error.is_connect()) => {
                    self.pause(Duration::from_secs(u64::from(attempt) * 2)).await?;
                    continue;
                }
                Err(error) => return Err(SourceError::Network(describe(&error))),
            };
            let status = response.status();
            let now = Timestamp::now();
            if let Some(wait) = rate_limit_wait(status, response.headers(), now) {
                if wait > self.max_wait || attempt >= MAX_ATTEMPTS {
                    let until = Timestamp(now.seconds().saturating_add(i64::try_from(wait.as_secs()).unwrap_or(i64::MAX)));
                    return Err(SourceError::RateLimited(until.iso8601()));
                }
                tracing::info!(seconds = wait.as_secs(), "import paused by the source's rate limit");
                self.pause(wait).await?;
                continue;
            }
            if status.is_server_error() && attempt < 3 {
                self.pause(Duration::from_secs(u64::from(attempt) * 2)).await?;
                continue;
            }
            if status.is_success() {
                if let Some(wait) = exhausted_until_reset(response.headers(), now).filter(|wait| *wait <= self.max_wait) {
                    tracing::info!(seconds = wait.as_secs(), "import pausing until the source's rate limit resets");
                    self.pause(wait).await?;
                }
                return Ok(response);
            }
            return Err(match status {
                StatusCode::UNAUTHORIZED => SourceError::Unauthorized,
                StatusCode::NOT_FOUND => SourceError::NotFound(what),
                _ => {
                    let body = response.text().await.unwrap_or_default();
                    SourceError::Status { status: status.as_u16(), message: error_message(&body) }
                }
            });
        }
    }

    async fn pause(&self, wait: Duration) -> Result<(), SourceError> {
        tokio::select! {
            () = self.cancel.cancelled() => Err(SourceError::Cancelled),
            () = tokio::time::sleep(wait) => Ok(()),
        }
    }

    fn api_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut request = self.api.get(url).header(ACCEPT, "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(authorization) = self.api_authorization() {
            request = request.header(AUTHORIZATION, authorization);
        }
        request
    }

    /// `GET url` (on the API) as JSON.
    pub async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T, SourceError> {
        let response = self.send(&self.api, || self.api_request(url)).await?;
        read_json(response).await
    }

    /// The repository's metadata.
    pub async fn repository(&self) -> Result<super::api::Repo, SourceError> {
        self.get(&self.source.repo_api("")).await
    }

    /// A user's public profile (for their public email address).
    pub async fn user(&self, login: &str) -> Result<super::api::User, SourceError> {
        let login: String = login.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
        self.get(&format!("{}/users/{login}", self.source.api_base)).await
    }

    /// Pages of the repository listing at `path` (e.g. `/labels`), with `query`.
    pub fn pages(&self, path: &str, query: &str) -> Pages {
        let separator = if query.is_empty() { "" } else { "&" };
        let url = format!("{}?{query}{separator}per_page={PER_PAGE}&limit={PER_PAGE}", self.source.repo_api(path));
        Pages { client: self.clone(), next: Some(url) }
    }

    /// Every item of the listing at `path`.
    pub async fn all<T: DeserializeOwned>(&self, path: &str, query: &str) -> Result<Vec<T>, SourceError> {
        let mut pages = self.pages(path, query);
        let mut items = Vec::new();
        while let Some(page) = pages.next::<T>().await? {
            items.extend(page);
        }
        Ok(items)
    }

    /// `POST url` with a JSON body (the LFS batch API), with `headers`; no redirects.
    pub async fn post_json<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &HeaderMap,
        body: &impl serde::Serialize,
    ) -> Result<T, SourceError> {
        let body = serde_json::to_vec(body).map_err(|e| SourceError::Invalid(e.to_string()))?;
        let response = self.send(&self.api, || self.api.post(url).headers(headers.clone()).body(body.clone())).await?;
        read_json(response).await
    }

    /// Starts downloading `url` with `headers`; the body is read through the returned
    /// [`Download`] as it arrives. With `api_auth`, the API token is added when `url` is
    /// on the API's origin (GitHub's release file URLs).
    pub async fn download(&self, url: &str, headers: &HeaderMap, api_auth: bool) -> Result<Download, SourceError> {
        let response = self
            .send(&self.downloads, || {
                let mut request = self.downloads.get(url).headers(headers.clone());
                if api_auth
                    && self.is_api_url(url)
                    && let Some(authorization) = self.api_authorization()
                {
                    request = request.header(AUTHORIZATION, authorization);
                }
                request
            })
            .await?;
        Ok(Download::start(response, self.cancel.clone()))
    }
}

/// A listing being read page by page.
#[derive(Debug)]
pub struct Pages {
    client: SourceClient,
    next: Option<String>,
}

impl Pages {
    /// The next page, or `None` after the last one.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<Option<Vec<T>>, SourceError> {
        let Some(url) = self.next.take() else { return Ok(None) };
        let response = self.client.send(&self.client.api, || self.client.api_request(&url)).await?;
        let next = next_link(response.headers());
        let items: Vec<T> = read_json(response).await?;
        // A `next` link elsewhere would carry the token there; an empty page ends the
        // listing even if the server keeps linking (some Gitea versions do).
        self.next = next.filter(|next| self.client.is_api_url(next) && !items.is_empty());
        Ok(Some(items))
    }
}

/// Reads a JSON body.
async fn read_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, SourceError> {
    let body = response.bytes().await.map_err(|e| SourceError::Network(describe(&e)))?;
    serde_json::from_slice(&body).map_err(|e| SourceError::Invalid(e.to_string()))
}

/// The URL of `rel="next"` in a `Link` header.
pub fn next_link(headers: &HeaderMap) -> Option<String> {
    headers.get_all("link").iter().filter_map(|value| value.to_str().ok()).flat_map(|value| value.split(',')).find_map(|part| {
        let (target, params) = part.split_once(';')?;
        let is_next = params.split(';').any(|param| {
            let param = param.trim();
            param.strip_prefix("rel=").is_some_and(|rel| rel.trim_matches('"').split_whitespace().any(|r| r == "next"))
        });
        is_next.then(|| target.trim().trim_start_matches('<').trim_end_matches('>').to_owned())
    })
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// How long to wait before retrying a response refused by a rate limit, if it was one.
pub fn rate_limit_wait(status: StatusCode, headers: &HeaderMap, now: Timestamp) -> Option<Duration> {
    if !matches!(status, StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS) {
        return None;
    }
    if let Some(seconds) = header_number(headers, RETRY_AFTER.as_str()) {
        return Some(Duration::from_secs(u64::try_from(seconds).unwrap_or(0).max(1)));
    }
    if header_number(headers, "x-ratelimit-remaining") == Some(0) {
        let reset = header_number(headers, "x-ratelimit-reset")?;
        return Some(Duration::from_secs(u64::try_from(reset - now.seconds()).unwrap_or(0) + 1));
    }
    // A secondary limit without a hint: GitHub asks for at least a minute.
    (status == StatusCode::TOO_MANY_REQUESTS).then_some(Duration::from_secs(60))
}

/// After a successful response that used up the budget: how long until it resets.
pub fn exhausted_until_reset(headers: &HeaderMap, now: Timestamp) -> Option<Duration> {
    if header_number(headers, "x-ratelimit-remaining") != Some(0) {
        return None;
    }
    let reset = header_number(headers, "x-ratelimit-reset")?;
    (reset >= now.seconds()).then(|| Duration::from_secs(u64::try_from(reset - now.seconds()).unwrap_or(0) + 1))
}

/// The `message` of a JSON error body, or the start of the body.
fn error_message(body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("message").and_then(serde_json::Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| body.to_owned());
    rbit_core::import::truncate_chars(message.trim(), MAX_ERROR_BODY).to_owned()
}

/// A transport error in words (the guard's refusal, a timeout, a TLS failure…).
fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        message = format!("{message}: {inner}");
        source = inner.source();
    }
    message
}

// ------------------------------------------------------------------ streamed downloads

/// A response body read as it arrives. A failure while receiving is a read error, so a
/// truncated download can never pass for a complete one.
#[derive(Debug)]
pub struct Download {
    receiver: mpsc::Receiver<io::Result<Vec<u8>>>,
    chunk: Vec<u8>,
    offset: usize,
    /// The announced length, if any.
    pub content_length: Option<u64>,
}

impl Download {
    fn start(mut response: reqwest::Response, cancel: CancellationToken) -> Self {
        let content_length = response.content_length();
        let (sender, receiver) = mpsc::channel(4);
        tokio::spawn(async move {
            loop {
                let chunk = tokio::select! {
                    () = cancel.cancelled() => Err(io::Error::other("the import was stopped")),
                    chunk = response.chunk() => chunk.map_err(|e| io::Error::other(describe(&e))),
                };
                let item = match chunk {
                    Ok(Some(bytes)) => Ok(bytes.to_vec()),
                    Ok(None) => break,
                    Err(error) => Err(error),
                };
                let failed = item.is_err();
                if sender.send(item).await.is_err() || failed {
                    break;
                }
            }
        });
        Self { receiver, chunk: Vec::new(), offset: 0, content_length }
    }
}

impl AsyncRead for Download {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<io::Result<()>> {
        loop {
            if self.offset < self.chunk.len() {
                let available = &self.chunk[self.offset..];
                let take = available.len().min(buf.remaining());
                buf.put_slice(&available[..take]);
                self.offset += take;
                return Poll::Ready(Ok(()));
            }
            match self.receiver.poll_recv(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.chunk = chunk;
                    self.offset = 0;
                }
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Err(error)),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn next_links_are_found() {
        let link = r#"<https://api.github.com/repositories/1/issues?page=2>; rel="next", <https://api.github.com/repositories/1/issues?page=5>; rel="last""#;
        assert_eq!(next_link(&headers(&[("link", link)])).as_deref(), Some("https://api.github.com/repositories/1/issues?page=2"));
        let last_page = r#"<https://x/issues?page=1>; rel="first", <https://x/issues?page=4>; rel="prev""#;
        assert_eq!(next_link(&headers(&[("link", last_page)])), None);
        assert_eq!(next_link(&HeaderMap::new()), None);
        assert_eq!(next_link(&headers(&[("link", "<https://x/2>;rel=next")])).as_deref(), Some("https://x/2"), "Gitea spacing");
    }

    #[test]
    fn rate_limits_are_waited_out() {
        let now = Timestamp(1_000);
        let used_up = headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1030")]);
        assert_eq!(rate_limit_wait(StatusCode::FORBIDDEN, &used_up, now), Some(Duration::from_secs(31)));
        assert_eq!(rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &headers(&[("retry-after", "7")]), now), Some(Duration::from_secs(7)));
        assert_eq!(rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), now), Some(Duration::from_secs(60)));
        assert_eq!(rate_limit_wait(StatusCode::FORBIDDEN, &HeaderMap::new(), now), None, "a plain 403 is a permission problem");
        assert_eq!(rate_limit_wait(StatusCode::OK, &used_up, now), None);
        assert_eq!(exhausted_until_reset(&used_up, now), Some(Duration::from_secs(31)));
        assert_eq!(exhausted_until_reset(&headers(&[("x-ratelimit-remaining", "4")]), now), None);
    }

    #[test]
    fn error_bodies_are_summarised() {
        assert_eq!(error_message(r#"{"message": "Bad credentials", "documentation_url": "x"}"#), "Bad credentials");
        assert_eq!(error_message("plain text"), "plain text");
        assert_eq!(error_message(&"x".repeat(1000)).len(), MAX_ERROR_BODY);
    }
}
