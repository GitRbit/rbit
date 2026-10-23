//! Cached upstream release notices. Rendering never performs network I/O.
//!
//! The fixed GitHub endpoint receives no instance identity, credentials or running
//! version. Only published stable releases participate in `SemVer` precedence.

use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rbit_core::config::UpdateConfig;
use reqwest::{Client, header};
use semver::Version;
use serde::Deserialize;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const SOURCE: &str = "https://api.github.com/repos/GitRbit/rbit/releases/latest";
const RELEASES: &str = "https://github.com/GitRbit/rbit/releases/tag/";
const USER_AGENT: &str = "rbit-update-check";
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const CACHE_TIME: Duration = Duration::from_hours(24);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A validated public release. Its link always belongs to the fixed upstream repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableRelease {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState {
    Disabled,
    Pending,
    UpToDate,
    Available,
    Unavailable,
}

/// A cheap snapshot of the last check, shared by the footer and administrator dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    /// UTC nanoseconds of the last observation, ordered across page caches and restarts.
    pub revision: u128,
    pub state: UpdateState,
    pub release: Option<AvailableRelease>,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self { revision: 0, state: UpdateState::Disabled, release: None }
    }
}

impl UpdateStatus {
    /// Versionless browser notice metadata, independent of current-version reporting.
    pub fn notice(&self) -> &str {
        self.release.as_ref().map_or_else(
            || match self.state {
                UpdateState::Disabled | UpdateState::UpToDate => "none",
                _ => "pending",
            },
            |release| release.url.as_str(),
        )
    }

    pub const fn label(&self) -> &'static str {
        match self.state {
            UpdateState::Disabled => "Automatic update checks are disabled.",
            UpdateState::Pending => "Waiting for the first release check.",
            UpdateState::UpToDate => "No newer stable release is available.",
            UpdateState::Available => "A newer stable release is available.",
            UpdateState::Unavailable => "The last release check was unavailable. Retrying automatically.",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn newer_release(release: Release, running: &str) -> Result<Option<AvailableRelease>, &'static str> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let tag = release.tag_name;
    if tag.len() > 128 || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')) {
        return Err("invalid release tag");
    }
    let candidate = Version::parse(tag.strip_prefix('v').unwrap_or(&tag)).map_err(|_| "invalid release version")?;
    let current = Version::parse(running).map_err(|_| "invalid running version")?;
    if !candidate.pre.is_empty() || !candidate.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    Ok(Some(AvailableRelease { version: candidate.to_string(), url: format!("{RELEASES}{tag}") }))
}

#[derive(Debug, Clone)]
pub struct UpdateService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    enabled: bool,
    http: Option<Client>,
    source: String,
    status: Mutex<UpdateStatus>,
    started: AtomicBool,
    cancel: CancellationToken,
    tasks: TaskTracker,
}

impl UpdateService {
    pub(crate) fn new(config: &UpdateConfig) -> Self {
        crate::webhooks::delivery::install_crypto_provider();
        let http = if config.check_for_updates { build_client().ok() } else { None };
        Self::from_client(http, SOURCE.to_owned(), config.check_for_updates)
    }

    fn from_client(http: Option<Client>, source: String, enabled: bool) -> Self {
        let state = if !enabled {
            UpdateState::Disabled
        } else if http.is_some() {
            UpdateState::Pending
        } else {
            UpdateState::Unavailable
        };
        Self {
            inner: Arc::new(Inner {
                enabled,
                http,
                source,
                status: Mutex::new(UpdateStatus { revision: next_revision(0), state, release: None }),
                started: AtomicBool::new(false),
                cancel: CancellationToken::new(),
                tasks: TaskTracker::new(),
            }),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.inner.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Starts one background checker. Disabled instances create no HTTP client or task.
    pub fn spawn_background(&self) {
        if !self.inner.enabled || self.inner.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        self.inner.tasks.spawn(async move {
            service.run().await;
        });
    }

    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();
        self.inner.tasks.close();
        let _ = tokio::time::timeout(REQUEST_TIMEOUT, self.inner.tasks.wait()).await;
    }

    async fn run(&self) {
        let mut failures = 0_u32;
        loop {
            let result = tokio::select! {
                () = self.inner.cancel.cancelled() => return,
                result = self.check() => result,
            };
            let delay = match result {
                Ok(release) => {
                    failures = 0;
                    let state = if release.is_some() { UpdateState::Available } else { UpdateState::UpToDate };
                    let mut status = self.inner.status.lock().unwrap_or_else(PoisonError::into_inner);
                    *status = UpdateStatus { revision: next_revision(status.revision), state, release };
                    CACHE_TIME
                }
                Err(retry) => {
                    failures = failures.saturating_add(1);
                    let mut status = self.inner.status.lock().unwrap_or_else(PoisonError::into_inner);
                    status.state = UpdateState::Unavailable;
                    status.revision = next_revision(status.revision);
                    // Retain the last successful notice during an upstream outage.
                    failure_delay(failures).max(retry.unwrap_or_default()).min(CACHE_TIME)
                }
            };
            tokio::select! {
                () = self.inner.cancel.cancelled() => return,
                () = tokio::time::sleep(delay) => {}
            }
        }
    }

    async fn check(&self) -> Result<Option<AvailableRelease>, Option<Duration>> {
        if !self.inner.enabled {
            return Ok(None);
        }
        // A failed optional client must not prevent the instance starting; retry building
        // it with the same backoff as network failures.
        let http = match &self.inner.http {
            Some(http) => http.clone(),
            None => build_client().map_err(|_| None)?,
        };
        let mut response = http
            .get(&self.inner.source)
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
        let release = serde_json::from_slice(&bytes).map_err(|_| None)?;
        newer_release(release, env!("CARGO_PKG_VERSION")).map_err(|_| None)
    }
}

fn next_revision(previous: u128) -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(previous.saturating_add(1), |elapsed| elapsed.as_nanos().max(previous.saturating_add(1)))
}

fn failure_delay(failures: u32) -> Duration {
    Duration::from_secs(15 * 60 * 2_u64.saturating_pow(failures.saturating_sub(1).min(7))).min(CACHE_TIME)
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
    use axum::{Json, Router, extract::Request, response::IntoResponse, routing::get};
    use reqwest::StatusCode;
    use std::sync::atomic::AtomicUsize;

    fn release(tag: &str) -> Release {
        Release { tag_name: tag.into(), draft: false, prerelease: false }
    }

    #[test]
    fn release_precedence_ignores_build_metadata_and_rejects_unsafe_tags() {
        for (tag, current, newer) in [
            ("v1.10.0", "1.9.0", true),
            ("1.9.0", "1.10.0", false),
            ("v1.0.0", "1.0.0-rc.1", true),
            ("v1.0.0", "1.1.0-rc.1", false),
            ("v1.0.0+upstream", "1.0.0+local", false),
            ("v2.0.0-rc.1", "1.0.0", false),
        ] {
            assert_eq!(newer_release(release(tag), current).unwrap().is_some(), newer, "{tag} vs {current}");
        }
        for tag in ["../../evil", "https://evil.example", "v1.0", "v1.0.0\n"] {
            assert!(newer_release(release(tag), "0.1.0").is_err());
        }
        let mut draft = release("v99.0.0");
        draft.draft = true;
        assert!(newer_release(draft, "0.1.0").unwrap().is_none());
        let mut preview = release("v99.0.0");
        preview.prerelease = true;
        assert!(newer_release(preview, "0.1.0").unwrap().is_none());
        assert_eq!(failure_delay(1), Duration::from_mins(15));
        assert_eq!(failure_delay(2), Duration::from_mins(30));
        assert_eq!(failure_delay(u32::MAX), CACHE_TIME);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, "3600".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_hours(1)));
    }

    #[tokio::test]
    async fn worker_caches_checks_sends_no_running_version_and_stops_promptly() {
        let requests = Arc::new(AtomicUsize::new(0));
        let observed = requests.clone();
        let app = Router::new().route(
            "/latest",
            get(move |request: Request| {
                observed.fetch_add(1, Ordering::SeqCst);
                async move {
                    assert_eq!(request.headers()[header::USER_AGENT], USER_AGENT);
                    for key in [header::AUTHORIZATION, header::COOKIE, header::REFERER] {
                        assert!(request.headers().get(key).is_none());
                    }
                    assert!(request.uri().query().is_none());
                    Json(serde_json::json!({ "tag_name":"v99.0.0", "draft":false, "prerelease":false, "html_url":"https://evil.example" }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = Client::builder().user_agent(USER_AGENT).no_proxy().timeout(REQUEST_TIMEOUT).build().unwrap();
        let service = UpdateService::from_client(Some(client), format!("http://{address}/latest"), true);
        let initial_revision = service.status().revision;
        service.spawn_background();
        service.spawn_background();
        tokio::time::timeout(Duration::from_secs(5), async {
            while service.status().state != UpdateState::Available {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(service.status().revision > initial_revision);
        let observed_revision = service.status().revision;
        assert_eq!(service.status().revision, observed_revision, "reading a snapshot does not advance its revision");
        assert_eq!(service.status().release.unwrap().url, "https://github.com/GitRbit/rbit/releases/tag/v99.0.0");
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        tokio::time::timeout(Duration::from_secs(1), service.shutdown()).await.unwrap();
        let disabled = UpdateService::new(&UpdateConfig { check_for_updates: false, report_version: true });
        disabled.spawn_background();
        assert_eq!(disabled.status().state, UpdateState::Disabled);
        assert!(disabled.inner.http.is_none());
        server.abort();
    }

    #[tokio::test]
    async fn upstream_redirects_oversized_json_and_timeouts_do_not_produce_notices() {
        let app = Router::new()
            .route("/redirect", get(|| async { (StatusCode::FOUND, [(header::LOCATION, "https://evil.example")]) }))
            .route("/large", get(|| async { "x".repeat(MAX_RESPONSE_BYTES + 1) }))
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    StatusCode::OK.into_response()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        for path in ["redirect", "large", "slow"] {
            let client = Client::builder()
                .user_agent(USER_AGENT)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_millis(500))
                .build()
                .unwrap();
            let service = UpdateService::from_client(Some(client), format!("http://{address}/{path}"), true);
            assert!(service.check().await.is_err());
            assert!(service.status().release.is_none());
        }
        server.abort();
    }
}
