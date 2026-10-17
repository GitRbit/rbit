//! Importing a repository from GitHub (or GitHub Enterprise) and Gitea/Forgejo: where it
//! comes from, what to bring along, and how far a running import got.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! An import copies the Git repository (branches, tags and pull request heads) and,
//! optionally, labels, milestones, issues and pull requests with their comments, releases
//! with their files, and Git LFS objects, through the source's REST API. Gitea and Forgejo
//! serve a GitHub-like API under `/api/v1`, so one client reads both. Everything here is
//! pure: parsing user input ([`ImportSource::parse`]), the stored options and progress,
//! and the small conversions the importer needs (timestamps, label colours, text that
//! must fit rbit's limits).

use std::fmt;

use serde::{Deserialize, Serialize};
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

use crate::{AccountId, RepoId, Timestamp, mirror::MirrorUrl};

/// Longest owner or repository name accepted from the source.
const MAX_SOURCE_NAME_LEN: usize = 100;
/// Warnings kept per import (the rest are counted, not stored).
pub const MAX_IMPORT_WARNINGS: usize = 50;
/// Longest warning or error message kept.
pub const MAX_IMPORT_MESSAGE_CHARS: usize = 500;

/// The kind of forge an import reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// github.com (API at `https://api.github.com`).
    #[default]
    GitHub,
    /// GitHub Enterprise Server at a given host (API at `<host>/api/v3`).
    GitHubEnterprise,
    /// Gitea or Forgejo at a given host (API at `<host>/api/v1`).
    Gitea,
}

impl SourceKind {
    pub const ALL: [Self; 3] = [Self::GitHub, Self::GitHubEnterprise, Self::Gitea];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitHubEnterprise => "github_enterprise",
            Self::Gitea => "gitea",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// How the kind is named in forms.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitHubEnterprise => "GitHub Enterprise",
            Self::Gitea => "Gitea or Forgejo",
        }
    }

    /// How the platform is named in attributions ("originally by @x on GitHub").
    pub const fn platform(self) -> &'static str {
        match self {
            Self::GitHub | Self::GitHubEnterprise => "GitHub",
            Self::Gitea => "Gitea",
        }
    }

    /// Whether the kind needs a host URL (everything but github.com).
    pub const fn needs_host(self) -> bool {
        !matches!(self, Self::GitHub)
    }
}

/// Why an import source was refused. Messages are shown to users as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportSourceError {
    #[error("enter the server's address, such as https://gitea.example.com")]
    MissingHost,
    #[error("the server address: {0}")]
    Host(String),
    #[error("enter the repository as owner/name or as its URL")]
    Repository,
}

/// A validated source: the API to read and the repository on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSource {
    pub kind: SourceKind,
    /// The API's base URL without a trailing slash, e.g. `https://api.github.com`.
    pub api_base: String,
    /// The web base URL without a trailing slash, e.g. `https://github.com`.
    pub web_base: String,
    /// Host name (or address literal) of the API, for the address check.
    pub api_host: String,
    pub api_port: u16,
    pub owner: String,
    pub name: String,
}

impl ImportSource {
    /// Validates the import form: the kind, the server address (ignored for github.com)
    /// and the repository as `owner/name` or as a URL (`https://host/owner/name(.git)`).
    pub fn parse(kind: SourceKind, host: &str, repository: &str) -> Result<Self, ImportSourceError> {
        let web_base = if kind.needs_host() {
            let host = host.trim().trim_end_matches('/');
            if host.is_empty() {
                return Err(ImportSourceError::MissingHost);
            }
            let url: MirrorUrl = host.parse().map_err(|e: crate::mirror::MirrorUrlError| ImportSourceError::Host(e.to_string()))?;
            url.as_str().trim_end_matches('/').to_owned()
        } else {
            "https://github.com".to_owned()
        };
        let api_base = match kind {
            SourceKind::GitHub => "https://api.github.com".to_owned(),
            SourceKind::GitHubEnterprise => format!("{web_base}/api/v3"),
            SourceKind::Gitea => format!("{web_base}/api/v1"),
        };
        let api: MirrorUrl = api_base.parse().map_err(|e: crate::mirror::MirrorUrlError| ImportSourceError::Host(e.to_string()))?;
        let (owner, name) = parse_repository(&web_base, repository).ok_or(ImportSourceError::Repository)?;
        Ok(Self { kind, api_host: api.host().to_owned(), api_port: api.port(), api_base, web_base, owner, name })
    }

    /// `owner/name` on the source.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// The repository's API URL followed by `path` (which starts with `/` or is empty).
    pub fn repo_api(&self, path: &str) -> String {
        format!("{}/repos/{}/{}{path}", self.api_base, self.owner, self.name)
    }

    /// The repository's web page.
    pub fn web_url(&self) -> String {
        format!("{}/{}/{}", self.web_base, self.owner, self.name)
    }
}

/// A source name segment: what GitHub and Gitea allow in owners and repositories.
fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= MAX_SOURCE_NAME_LEN
        && segment != "."
        && segment != ".."
        && segment.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// `owner/name`, `owner/name.git` or `<web_base or any http(s) URL>/owner/name[.git][/…]`.
fn parse_repository(web_base: &str, input: &str) -> Option<(String, String)> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let path = if lower.starts_with("https://") || lower.starts_with("http://") {
        let rest = &input[input.find("://")? + 3..];
        let path = rest.split_once('/').map_or("", |(_, path)| path);
        // A URL on another host than the chosen server is a mistake worth refusing.
        let base_host = web_base.split_once("://").map_or(web_base, |(_, rest)| rest).split('/').next().unwrap_or_default();
        let host = rest.split('/').next().unwrap_or_default();
        if !host.eq_ignore_ascii_case(base_host) {
            return None;
        }
        // A server under a path prefix (`https://host/gitea`) keeps it in front.
        let prefix = web_base.split_once("://").map_or("", |(_, rest)| rest).split_once('/').map_or("", |(_, prefix)| prefix);
        path.strip_prefix(prefix).unwrap_or(path).trim_start_matches('/')
    } else {
        input
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let owner = segments.next()?;
    let name = segments.next()?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    // `owner/name` alone; a URL may continue (`/issues`, `/tree/main`).
    if !lower.starts_with("http") && segments.next().is_some() {
        return None;
    }
    (valid_segment(owner) && valid_segment(name)).then(|| (owner.to_owned(), name.to_owned()))
}

// ------------------------------------------------------------------ options and progress

/// What an import brings along besides the Git repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "one checkbox each on the import form")]
pub struct ImportOptions {
    /// Issues and pull requests with their comments (pull request heads are fetched as
    /// `refs/pull/{n}/head`).
    pub issues: bool,
    pub labels: bool,
    pub milestones: bool,
    /// Releases with their attached files.
    pub releases: bool,
    /// Git LFS objects referenced by the repository's history.
    pub lfs: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { issues: true, labels: true, milestones: true, releases: true, lfs: true }
    }
}

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl ImportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Queued, Self::Running, Self::Succeeded, Self::Failed].into_iter().find(|status| status.as_str() == value)
    }

    /// Queued or running.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// The step an import is working on (or stopped at).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImportPhase {
    Repository,
    Lfs,
    Labels,
    Milestones,
    Issues,
    Releases,
    Finished,
}

impl ImportPhase {
    pub const ALL: [Self; 7] = [Self::Repository, Self::Lfs, Self::Labels, Self::Milestones, Self::Issues, Self::Releases, Self::Finished];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Lfs => "lfs",
            Self::Labels => "labels",
            Self::Milestones => "milestones",
            Self::Issues => "issues",
            Self::Releases => "releases",
            Self::Finished => "finished",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|phase| phase.as_str() == value)
    }

    /// Shown on the progress page.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Repository => "Copying the Git repository",
            Self::Lfs => "Copying Git LFS objects",
            Self::Labels => "Importing labels",
            Self::Milestones => "Importing milestones",
            Self::Issues => "Importing issues and pull requests",
            Self::Releases => "Importing releases",
            Self::Finished => "Finished",
        }
    }
}

/// How much an import has brought over so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportCounts {
    pub labels: u64,
    pub milestones: u64,
    pub issues: u64,
    pub pulls: u64,
    pub comments: u64,
    pub releases: u64,
    pub assets: u64,
    pub lfs_objects: u64,
    /// Warnings beyond [`MAX_IMPORT_WARNINGS`] that were not stored.
    pub dropped_warnings: u64,
}

/// An import as stored: one per repository (the latest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoImport {
    pub repo_id: RepoId,
    /// The user who started it; `None` once their account is deleted.
    pub importer: Option<AccountId>,
    pub kind: SourceKind,
    /// The source repository's web page.
    pub source_url: String,
    pub options: ImportOptions,
    pub status: ImportStatus,
    pub phase: ImportPhase,
    pub counts: ImportCounts,
    /// Problems with single items (a release file, an issue) that did not stop the import.
    pub warnings: Vec<String>,
    /// Why the import stopped, when it failed.
    pub error: Option<String>,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

// ------------------------------------------------------------------ conversions

/// Parses an RFC 3339 timestamp as GitHub and Gitea write them (`2024-01-02T15:04:05Z`,
/// optional fractional seconds, `Z` or a `±hh:mm` offset). `None` for anything else.
pub fn parse_timestamp(value: &str) -> Option<Timestamp> {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let number = |range: std::ops::Range<usize>| value.get(range)?.parse::<u16>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let mut rest = &value[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => UtcOffset::UTC,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (hours, minutes) = rest[1..].split_once(':')?;
            let (hours, minutes) = (hours.parse::<i8>().ok()?, minutes.parse::<i8>().ok()?);
            UtcOffset::from_hms(sign * hours, sign * minutes, 0).ok()?
        }
    };
    let date = Date::from_calendar_date(i32::from(year), Month::try_from(u8::try_from(month).ok()?).ok()?, u8::try_from(day).ok()?).ok()?;
    let time = Time::from_hms(u8::try_from(hour).ok()?, u8::try_from(minute).ok()?, u8::try_from(second).ok()?).ok()?;
    Some(Timestamp(PrimitiveDateTime::new(date, time).assume_offset(offset).unix_timestamp()))
}

/// A label colour as `#rrggbb` in lower case, from `rrggbb` (GitHub) or `#rrggbb` (Gitea).
pub fn label_color(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// The longest prefix of `text` that fits in `max_bytes` without splitting a character.
pub fn truncate_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The first `max_chars` characters of `text`.
pub fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices().nth(max_chars).map_or(text, |(index, _)| &text[..index])
}

/// A message trimmed to [`MAX_IMPORT_MESSAGE_CHARS`] without control characters other than
/// newlines, for warnings and errors shown on the progress page.
pub fn clean_message(message: &str) -> String {
    message.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_IMPORT_MESSAGE_CHARS).collect()
}

impl fmt::Display for ImportPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_parsed_from_names_and_urls() {
        let github = ImportSource::parse(SourceKind::GitHub, "ignored", " rust-lang/rust ").unwrap();
        assert_eq!(github.api_base, "https://api.github.com");
        assert_eq!(github.repo_api("/issues"), "https://api.github.com/repos/rust-lang/rust/issues");
        assert_eq!(github.web_url(), "https://github.com/rust-lang/rust");
        assert_eq!((github.api_host.as_str(), github.api_port), ("api.github.com", 443));

        let from_url = ImportSource::parse(SourceKind::GitHub, "", "https://github.com/rust-lang/rust.git").unwrap();
        assert_eq!(from_url.full_name(), "rust-lang/rust");
        let deep = ImportSource::parse(SourceKind::GitHub, "", "https://github.com/a/b/tree/main").unwrap();
        assert_eq!(deep.full_name(), "a/b");

        let gitea = ImportSource::parse(SourceKind::Gitea, "https://codeberg.org/", "forgejo/forgejo").unwrap();
        assert_eq!(gitea.api_base, "https://codeberg.org/api/v1");
        assert_eq!(gitea.web_base, "https://codeberg.org");
        let enterprise = ImportSource::parse(SourceKind::GitHubEnterprise, "http://127.0.0.1:8080", "a/b").unwrap();
        assert_eq!(enterprise.api_base, "http://127.0.0.1:8080/api/v3");
        assert_eq!((enterprise.api_host.as_str(), enterprise.api_port), ("127.0.0.1", 8080));
        let prefixed = ImportSource::parse(SourceKind::Gitea, "https://example.com/git", "https://example.com/git/a/b").unwrap();
        assert_eq!((prefixed.api_base.as_str(), prefixed.full_name().as_str()), ("https://example.com/git/api/v1", "a/b"));
    }

    #[test]
    fn bad_sources_are_refused() {
        assert_eq!(ImportSource::parse(SourceKind::Gitea, " ", "a/b"), Err(ImportSourceError::MissingHost));
        assert!(matches!(ImportSource::parse(SourceKind::Gitea, "ftp://x", "a/b"), Err(ImportSourceError::Host(_))));
        assert!(matches!(ImportSource::parse(SourceKind::Gitea, "https://u:p@x", "a/b"), Err(ImportSourceError::Host(_))));
        for repository in ["", "a", "a/", "/b", "a/b/c", "a/../b", "a b/c", "https://gitlab.com/a/b", "a/..", "../b"] {
            assert_eq!(ImportSource::parse(SourceKind::GitHub, "", repository), Err(ImportSourceError::Repository), "{repository:?}");
        }
    }

    #[test]
    fn timestamps_are_parsed() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00Z"), Some(Timestamp(0)));
        assert_eq!(parse_timestamp("2011-01-26T19:01:12Z"), Some(Timestamp(1_296_068_472)));
        assert_eq!(parse_timestamp("2011-01-26T19:01:12.123456Z"), Some(Timestamp(1_296_068_472)));
        assert_eq!(parse_timestamp("2011-01-27T03:01:12+08:00"), Some(Timestamp(1_296_068_472)));
        assert_eq!(parse_timestamp("2011-01-26T17:31:12-01:30"), Some(Timestamp(1_296_068_472)));
        for bad in ["", "2011-01-26", "2011-13-26T19:01:12Z", "2011-01-26T25:01:12Z", "2011-01-26T19:01:12", "2011-01-26T19:01:12.Z", "x"] {
            assert_eq!(parse_timestamp(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn colours_and_text_are_normalised() {
        assert_eq!(label_color("F29513").as_deref(), Some("#f29513"));
        assert_eq!(label_color("#e11d21").as_deref(), Some("#e11d21"));
        assert_eq!(label_color("red"), None);
        assert_eq!(label_color("#12345"), None);
        assert_eq!(truncate_bytes("héllo", 2), "h");
        assert_eq!(truncate_bytes("héllo", 3), "hé");
        assert_eq!(truncate_bytes("abc", 10), "abc");
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(clean_message(" a\u{7}b\nc "), "ab\nc");
    }

    #[test]
    fn statuses_and_phases_round_trip() {
        for status in [ImportStatus::Queued, ImportStatus::Running, ImportStatus::Succeeded, ImportStatus::Failed] {
            assert_eq!(ImportStatus::parse(status.as_str()), Some(status));
        }
        for phase in ImportPhase::ALL {
            assert_eq!(ImportPhase::parse(phase.as_str()), Some(phase));
        }
        for kind in SourceKind::ALL {
            assert_eq!(SourceKind::parse(kind.as_str()), Some(kind));
        }
        assert!(ImportStatus::Running.is_active() && !ImportStatus::Failed.is_active());
    }
}
