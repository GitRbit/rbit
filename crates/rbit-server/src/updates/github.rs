//! The production [`ReleaseSource`]: the fixed upstream GitHub release feed.
//!
//! The request carries a constant versionless User-Agent and no credentials, cookies,
//! instance identity or query. The client honors deployment proxy settings, requires
//! HTTPS, refuses redirects and bounds connect time, total time and body size.

use std::time::Duration;

use reqwest::{Client, header};

use super::{
    ReleaseSource,
    release::{CACHE_TIME, Release},
};

const SOURCE: &str = "https://api.github.com/repos/GitRbit/rbit/releases/latest";
pub(super) const USER_AGENT: &str = "rbit-update-check";
pub(super) const MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(super) struct GithubReleases {
    http: Option<Client>,
    source: String,
}

impl GithubReleases {
    pub(super) fn new() -> Self {
        rbit_services::webhooks::delivery::install_crypto_provider();
        Self { http: build_client().ok(), source: SOURCE.to_owned() }
    }

    /// An injected client and address, for local HTTP fixtures.
    #[cfg(test)]
    pub(super) fn with_client(http: Client, source: String) -> Self {
        Self { http: Some(http), source }
    }

    /// Whether the start-up client was built; a missing one is rebuilt on each check.
    pub(super) const fn ready(&self) -> bool {
        self.http.is_some()
    }

    async fn fetch(&self) -> Result<Release, Option<Duration>> {
        // Retry building a failed optional client with the same backoff as network failures.
        let http = match &self.http {
            Some(http) => http.clone(),
            None => build_client().map_err(|_| None)?,
        };
        let mut response = http
            .get(&self.source)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|_| None)?;
        if !response.status().is_success() {
            return Err(retry_after(response.headers()));
        }
        if response.content_length().is_some_and(|size| size > MAX_RESPONSE_BYTES as u64) {
            return Err(None);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| None)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(None);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| None)
    }
}

impl ReleaseSource for GithubReleases {
    fn latest(&self) -> impl Future<Output = Result<Release, Option<Duration>>> + Send {
        self.fetch()
    }
}

fn build_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent(USER_AGENT)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(1)
        .build()
}

fn retry_after(headers: &header::HeaderMap) -> Option<Duration> {
    let retry = headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|at| Duration::from_secs(at.saturating_sub(rbit_core::Timestamp::now().0.cast_unsigned())));
    retry.into_iter().chain(reset).max().map(|delay| delay.min(CACHE_TIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_hints_are_parsed_and_bounded() {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, "3600".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_hours(1)));
        headers.insert(header::RETRY_AFTER, "999999".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(CACHE_TIME));
        assert_eq!(retry_after(&header::HeaderMap::new()), None);
        assert!(GithubReleases::new().ready());
    }
}
