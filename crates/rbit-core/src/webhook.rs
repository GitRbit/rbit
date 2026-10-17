//! Webhooks: HTTP callbacks for repository and organisation events.
//!
//! Hook ids, the event kinds a hook subscribes to, target URLs and delivery records live
//! here as validated domain types. Deliveries are kept away from the internal network
//! with [`crate::network::is_internal_address`] (re-exported here).
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.

use std::{
    fmt,
    net::{IpAddr, Ipv6Addr},
    str::FromStr,
};

pub use crate::network::is_internal_address;
use crate::{AccountId, RepoId, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WebhookId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeliveryId(pub i64);

/// What a delivery reports. Names and payloads follow GitHub's, so existing receivers work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WebhookEvent {
    /// Refs were pushed: one delivery per changed ref.
    Push,
    /// A repository was created, deleted or transferred.
    Repository,
    /// A release was published.
    Release,
    /// A repository was forked.
    Fork,
    /// An issue was opened, edited, closed, reopened, labelled, assigned, … (wave B, I).
    Issues,
    /// A comment on an issue or pull request was created, edited or deleted (wave B, I).
    IssueComment,
    /// A pull request was opened, synchronised, closed or merged, … (wave B, P).
    PullRequest,
    /// A pull request review was submitted, edited or dismissed (wave B, P).
    PullRequestReview,
    /// Sent once when a hook is created (and on request) to check the receiver.
    Ping,
}

impl WebhookEvent {
    /// The events a hook can subscribe to, in display order. `ping` is always sent.
    pub const SUBSCRIBABLE: [Self; 8] = [
        Self::Push,
        Self::Repository,
        Self::Release,
        Self::Fork,
        Self::Issues,
        Self::IssueComment,
        Self::PullRequest,
        Self::PullRequestReview,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Repository => "repository",
            Self::Release => "release",
            Self::Fork => "fork",
            Self::Issues => "issues",
            Self::IssueComment => "issue_comment",
            Self::PullRequest => "pull_request",
            Self::PullRequestReview => "pull_request_review",
            Self::Ping => "ping",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::SUBSCRIBABLE.into_iter().chain([Self::Ping]).find(|event| event.as_str() == value)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Push => "Pushes",
            Self::Repository => "Repositories",
            Self::Release => "Releases",
            Self::Fork => "Forks",
            Self::Issues => "Issues",
            Self::IssueComment => "Issue comments",
            Self::PullRequest => "Pull requests",
            Self::PullRequestReview => "Pull request reviews",
            Self::Ping => "Pings",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Push => "Branches and tags pushed, created or deleted.",
            Self::Repository => "Repositories created, deleted or transferred.",
            Self::Release => "Releases published.",
            Self::Fork => "Repositories forked.",
            Self::Issues => "Issues opened, edited, closed, reopened, labeled, assigned, locked or deleted.",
            Self::IssueComment => "Comments on issues and pull requests created, edited or deleted.",
            Self::PullRequest => "Pull requests opened, updated, closed, merged, labeled, assigned or given reviewers.",
            Self::PullRequestReview => "Pull request reviews submitted, edited or dismissed.",
            Self::Ping => "Sent when the webhook is created.",
        }
    }
}

/// Where a hook is installed. Organisation hooks fire for every repository of the
/// organisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookOwner {
    Repository(RepoId),
    Organization(AccountId),
}

/// The most recent delivery of a hook, for lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastDelivery {
    pub status: DeliveryStatus,
    pub response_status: Option<u16>,
    pub at: Timestamp,
}

/// A configured hook. The secret is write-only: only whether one is set is ever read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Webhook {
    pub id: WebhookId,
    pub owner: WebhookOwner,
    pub url: WebhookUrl,
    /// Subscribed events, sorted and without duplicates; never contains `ping`.
    pub events: Vec<WebhookEvent>,
    pub active: bool,
    pub has_secret: bool,
    pub created_by: Option<AccountId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub last_delivery: Option<LastDelivery>,
}

impl Webhook {
    pub fn subscribes_to(&self, event: WebhookEvent) -> bool {
        event == WebhookEvent::Ping || self.events.contains(&event)
    }
}

/// Where a delivery is in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeliveryStatus {
    /// Waiting for its first or next attempt.
    Pending,
    /// Claimed by the worker; an attempt is in flight.
    Delivering,
    /// The receiver answered 2xx.
    Succeeded,
    /// Every attempt failed.
    Failed,
}

impl DeliveryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivering => "delivering",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "delivering" => Some(Self::Delivering),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Delivering => "Delivering",
            Self::Succeeded => "Delivered",
            Self::Failed => "Failed",
        }
    }

    /// No further attempts will be made.
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// One event sent (or to be sent) to one hook, with the outcome of its latest attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub id: DeliveryId,
    pub hook_id: WebhookId,
    /// Random id sent as `X-Rbit-Delivery`, so receivers can de-duplicate.
    pub guid: String,
    pub event: WebhookEvent,
    /// The payload's `action` (`created`, `published`, …), if the event has one.
    pub action: Option<String>,
    /// The exact JSON body sent.
    pub payload: String,
    pub status: DeliveryStatus,
    pub attempts: u32,
    pub next_attempt_at: Option<Timestamp>,
    pub last_attempt_at: Option<Timestamp>,
    /// Request headers of the latest attempt (never contains the secret itself).
    pub request_headers: Vec<(String, String)>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    /// The start of the response body (at most 64 KiB, lossily decoded).
    pub response_body: Option<String>,
    pub duration_ms: Option<u64>,
    /// Why the latest attempt failed without a response (refused address, timeout, …).
    pub error: Option<String>,
    pub created_at: Timestamp,
}

/// Formats headers one per line as `Name: value`, the form deliveries store them in.
pub fn format_header_block(headers: &[(String, String)]) -> String {
    headers.iter().map(|(name, value)| format!("{name}: {value}")).collect::<Vec<_>>().join("\n")
}

/// The inverse of [`format_header_block`]; lines without a colon are skipped.
pub fn parse_header_block(block: &str) -> Vec<(String, String)> {
    block
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

// ------------------------------------------------------------------ target URLs

/// Why a webhook URL was refused. Messages are shown to users as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebhookUrlError {
    #[error("URL must not be empty")]
    Empty,
    #[error("URL must be at most {max} bytes")]
    TooLong { max: usize },
    #[error("URL must start with http:// or https://")]
    Scheme,
    #[error("URL must not contain a username or password; use a secret instead")]
    Credentials,
    #[error("URL must name a host")]
    Host,
    #[error("URL has an invalid port")]
    Port,
    #[error("URL must not contain spaces or control characters")]
    Characters,
}

/// An absolute `http`/`https` URL without credentials, at most [`WebhookUrl::MAX_LEN`]
/// bytes. Whether its host may be contacted is decided separately (at configuration time
/// for literal addresses, at delivery time for names).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WebhookUrl {
    url: String,
    /// The host as written, without brackets for IPv6 literals; lower-cased.
    host: String,
}

impl WebhookUrl {
    pub const MAX_LEN: usize = 2048;

    pub fn parse(value: &str) -> Result<Self, WebhookUrlError> {
        let url = value.trim();
        if url.is_empty() {
            return Err(WebhookUrlError::Empty);
        }
        if url.len() > Self::MAX_LEN {
            return Err(WebhookUrlError::TooLong { max: Self::MAX_LEN });
        }
        if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(WebhookUrlError::Characters);
        }
        let (scheme, rest) = url.split_once("://").ok_or(WebhookUrlError::Scheme)?;
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return Err(WebhookUrlError::Scheme);
        }
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return Err(WebhookUrlError::Credentials);
        }
        let (host, port) = split_host_port(authority)?;
        if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':')) {
            return Err(WebhookUrlError::Host);
        }
        if let Some(port) = port
            && !matches!(port.parse::<u16>(), Ok(1..))
        {
            return Err(WebhookUrlError::Port);
        }
        Ok(Self { url: url.to_owned(), host: host.to_ascii_lowercase() })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The host name or address literal (IPv6 without brackets).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The host when it is an IP address literal rather than a name.
    pub fn literal_ip(&self) -> Option<IpAddr> {
        self.host.parse().ok()
    }

    /// Whether the host is an internal address literal or a name that always means this
    /// machine (`localhost`, `*.localhost`), which configuration refuses outright.
    pub fn targets_internal_host(&self) -> bool {
        let host = self.host.trim_end_matches('.');
        host == "localhost" || host.ends_with(".localhost") || self.literal_ip().is_some_and(is_internal_address)
    }
}

impl fmt::Display for WebhookUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

impl FromStr for WebhookUrl {
    type Err = WebhookUrlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// `host[:port]` or `[v6][:port]`.
fn split_host_port(authority: &str) -> Result<(&str, Option<&str>), WebhookUrlError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or(WebhookUrlError::Host)?;
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(WebhookUrlError::Host);
        }
        return match after {
            "" => Ok((host, None)),
            _ => after.strip_prefix(':').map(|port| (host, Some(port))).ok_or(WebhookUrlError::Port),
        };
    }
    match authority.split_once(':') {
        Some((host, port)) => Ok((host, Some(port))),
        None => Ok((authority, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip() {
        for event in WebhookEvent::SUBSCRIBABLE.into_iter().chain([WebhookEvent::Ping]) {
            assert_eq!(WebhookEvent::parse(event.as_str()), Some(event));
        }
        assert_eq!(WebhookEvent::parse("issues"), Some(WebhookEvent::Issues));
        assert_eq!(WebhookEvent::parse("pull_request_review"), Some(WebhookEvent::PullRequestReview));
        assert_eq!(WebhookEvent::parse("pull_request_review_comment"), None, "out of scope in wave B");
        assert_eq!(WebhookEvent::parse("Push"), None);
        assert!(!WebhookEvent::SUBSCRIBABLE.contains(&WebhookEvent::Ping), "ping is implicit");
    }

    #[test]
    fn statuses_round_trip() {
        for status in [DeliveryStatus::Pending, DeliveryStatus::Delivering, DeliveryStatus::Succeeded, DeliveryStatus::Failed] {
            assert_eq!(DeliveryStatus::parse(status.as_str()), Some(status));
        }
        assert!(DeliveryStatus::Failed.is_final() && DeliveryStatus::Succeeded.is_final());
        assert!(!DeliveryStatus::Pending.is_final() && !DeliveryStatus::Delivering.is_final());
    }

    #[test]
    fn header_blocks_round_trip() {
        let headers = vec![("Content-Type".to_owned(), "application/json".to_owned()), ("X-Rbit-Event".to_owned(), "push".to_owned())];
        let block = format_header_block(&headers);
        assert_eq!(block, "Content-Type: application/json\nX-Rbit-Event: push");
        assert_eq!(parse_header_block(&block), headers);
        assert_eq!(
            parse_header_block("garbage\nDate: Mon, 01 Jan 2026 10:00:00 GMT"),
            [("Date".to_owned(), "Mon, 01 Jan 2026 10:00:00 GMT".to_owned())]
        );
    }

    #[test]
    fn accepts_plain_http_and_https_urls() {
        let url = WebhookUrl::parse("  https://Hooks.Example.com:8443/rbit?team=ops#x ").unwrap();
        assert_eq!(url.as_str(), "https://Hooks.Example.com:8443/rbit?team=ops#x");
        assert_eq!(url.host(), "hooks.example.com");
        assert_eq!(url.literal_ip(), None);
        assert!(!url.targets_internal_host());

        let v6 = WebhookUrl::parse("http://[2606:4700::1111]/hook").unwrap();
        assert_eq!(v6.literal_ip(), Some("2606:4700::1111".parse().unwrap()));
        assert_eq!(WebhookUrl::parse("HTTP://example.com").unwrap().host(), "example.com");
    }

    #[test]
    fn refuses_malformed_urls() {
        let cases = [
            ("", WebhookUrlError::Empty),
            ("ftp://example.com/", WebhookUrlError::Scheme),
            ("example.com/hook", WebhookUrlError::Scheme),
            ("javascript://alert(1)", WebhookUrlError::Scheme),
            ("https://user:pass@example.com/", WebhookUrlError::Credentials),
            ("https://token@example.com/", WebhookUrlError::Credentials),
            ("https:///path", WebhookUrlError::Host),
            ("https://exa mple.com/", WebhookUrlError::Characters),
            ("https://example.com:0/", WebhookUrlError::Port),
            ("https://example.com:99999/", WebhookUrlError::Port),
            ("https://example.com:http/", WebhookUrlError::Port),
            ("https://[::1/", WebhookUrlError::Host),
            ("https://[not-v6]/", WebhookUrlError::Host),
            ("https://ex%41mple.com/", WebhookUrlError::Host),
        ];
        for (input, expected) in cases {
            assert_eq!(WebhookUrl::parse(input), Err(expected), "{input}");
        }
        let long = format!("https://example.com/{}", "a".repeat(WebhookUrl::MAX_LEN));
        assert_eq!(WebhookUrl::parse(&long), Err(WebhookUrlError::TooLong { max: WebhookUrl::MAX_LEN }));
    }

    #[test]
    fn literal_internal_hosts_are_recognised() {
        for url in [
            "http://127.0.0.1:3000/",
            "http://localhost/",
            "http://api.localhost./",
            "http://[::1]/",
            "http://10.1.2.3/",
            "http://[::ffff:192.168.0.1]/",
        ] {
            assert!(WebhookUrl::parse(url).unwrap().targets_internal_host(), "{url}");
        }
        assert!(!WebhookUrl::parse("http://203.0.114.1/").unwrap().targets_internal_host());
        assert!(!WebhookUrl::parse("http://localhost.example.com/").unwrap().targets_internal_host());
    }
}
