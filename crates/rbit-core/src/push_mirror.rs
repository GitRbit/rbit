//! Push mirrors: remotes a repository pushes its branches and tags to.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! A push mirror is the reverse of a pull mirror ([`crate::mirror`]): after pushes to the
//! repository (debounced) and on an interval, rbit force-pushes every branch and tag to
//! the remote and deletes the remote's branches and tags that no longer exist here. The
//! remote is an `http`/`https` URL without credentials ([`PushMirrorUrl`]); the user name
//! and secret (a password or access token) are stored separately and, like webhook
//! secrets, are write-only: they are never shown again, returned or logged.

use std::fmt;

use crate::{AccountId, RepoId, Timestamp, mirror::MirrorUrl};

/// Push mirrors per repository.
pub const MAX_PUSH_MIRRORS: usize = 10;
/// Longest user name, in bytes.
pub const MAX_USERNAME_BYTES: usize = 255;
/// Longest secret, in bytes.
pub const MAX_SECRET_BYTES: usize = 1024;

/// A push mirror's database id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PushMirrorId(pub i64);

/// A push mirror as shown in settings. The secret is not part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMirror {
    pub id: PushMirrorId,
    pub repo_id: RepoId,
    pub url: String,
    /// The user name sent with the secret (may be empty: some forges ignore it).
    pub username: String,
    pub has_secret: bool,
    pub enabled: bool,
    /// Push after every push to the repository (debounced), besides the interval.
    pub sync_on_push: bool,
    pub next_sync_at: Timestamp,
    pub last_attempt_at: Option<Timestamp>,
    pub last_success_at: Option<Timestamp>,
    /// `None` when the last attempt succeeded (or none ran yet).
    pub last_error: Option<String>,
    pub created_by: Option<AccountId>,
    pub created_at: Timestamp,
}

/// Why a push mirror's settings were refused. Messages are shown to users as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PushMirrorError {
    #[error("the remote URL: {0}")]
    Url(String),
    #[error("the user name must be at most {MAX_USERNAME_BYTES} bytes without colons or control characters")]
    Username,
    #[error("the secret must be at most {MAX_SECRET_BYTES} bytes without control characters")]
    Secret,
}

/// A validated remote: an `http(s)` URL without credentials ([`MirrorUrl`]'s rules), or —
/// only where local URLs are allowed (tests) — a `file://` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMirrorUrl {
    url: String,
    /// `None` for `file://` URLs.
    remote: Option<MirrorUrl>,
}

impl PushMirrorUrl {
    /// Validates `input`; `allow_local` admits `file://` URLs (a test switch: a local URL
    /// would let repository administrators write anywhere the server may).
    pub fn parse(input: &str, allow_local: bool) -> Result<Self, PushMirrorError> {
        let input = input.trim();
        if allow_local && input.to_ascii_lowercase().starts_with("file://") && !input.chars().any(char::is_control) {
            return Ok(Self { url: input.to_owned(), remote: None });
        }
        let remote: MirrorUrl = input.parse().map_err(|e: crate::mirror::MirrorUrlError| {
            let message = match e {
                crate::mirror::MirrorUrlError::Credentials => "put the user name and secret in their own fields".to_owned(),
                other => other.to_string(),
            };
            PushMirrorError::Url(message)
        })?;
        Ok(Self { url: remote.as_str().to_owned(), remote: Some(remote) })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The host and port to check before pushing; `None` for local URLs.
    pub fn remote(&self) -> Option<&MirrorUrl> {
        self.remote.as_ref()
    }
}

impl fmt::Display for PushMirrorUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

/// Checks a user name for HTTP Basic authentication (no `:`, no control characters).
pub fn validate_username(username: &str) -> Result<(), PushMirrorError> {
    let valid = username.len() <= MAX_USERNAME_BYTES && !username.contains(':') && !username.chars().any(char::is_control);
    if valid { Ok(()) } else { Err(PushMirrorError::Username) }
}

/// Checks a secret (password or token).
pub fn validate_secret(secret: &str) -> Result<(), PushMirrorError> {
    let valid = secret.len() <= MAX_SECRET_BYTES && !secret.chars().any(char::is_control);
    if valid { Ok(()) } else { Err(PushMirrorError::Secret) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_are_validated() {
        let url = PushMirrorUrl::parse(" https://github.com/acme/rocket.git ", false).unwrap();
        assert_eq!(url.as_str(), "https://github.com/acme/rocket.git");
        assert_eq!(url.remote().map(MirrorUrl::host), Some("github.com"));
        assert!(
            matches!(PushMirrorUrl::parse("https://u:p@github.com/a.git", false), Err(PushMirrorError::Url(m)) if m.contains("own fields"))
        );
        assert!(PushMirrorUrl::parse("file:///srv/git/a.git", false).is_err(), "local URLs need the switch");
        let local = PushMirrorUrl::parse("file:///srv/git/a.git", true).unwrap();
        assert_eq!((local.as_str(), local.remote()), ("file:///srv/git/a.git", None));
        assert!(PushMirrorUrl::parse("ssh://git@example.com/a", true).is_err());
    }

    #[test]
    fn credentials_are_validated() {
        assert!(validate_username("").is_ok() && validate_username("x-access-token").is_ok());
        assert!(validate_username("a:b").is_err() && validate_username("a\nb").is_err());
        assert!(validate_username(&"a".repeat(256)).is_err());
        assert!(validate_secret("ghp_123").is_ok() && validate_secret("a\rb").is_err());
        assert!(validate_secret(&"a".repeat(1025)).is_err());
    }
}
