//! Pull mirrors: repositories that periodically fetch from an upstream URL.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! A mirror is an ordinary repository with a [`Mirror`] row: Rustit fetches the
//! upstream's branches and tags on a schedule and nobody pushes to it. Upstreams are
//! `http`/`https` URLs without credentials ([`MirrorUrl`]); before every fetch the host's
//! addresses are checked with [`crate::network::is_internal_address`] unless the instance
//! allows private addresses, so a mirror cannot be used to probe the server's internal
//! network.

use std::{fmt, str::FromStr};

use crate::{RepoId, Timestamp};

/// Longest accepted upstream URL, in bytes.
pub const MAX_MIRROR_URL_LEN: usize = 2048;

/// A mirror's settings and the outcome of its last fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mirror {
    pub repo_id: RepoId,
    pub url: MirrorUrl,
    pub interval_minutes: u64,
    pub next_sync_at: Timestamp,
    pub last_sync_at: Option<Timestamp>,
    /// `None` when the last fetch succeeded (or none ran yet).
    pub last_error: Option<String>,
    pub created_at: Timestamp,
}

/// A validated upstream URL: `http` or `https`, a host, no user name or password, no
/// whitespace or control characters, at most [`MAX_MIRROR_URL_LEN`] bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorUrl {
    url: String,
    host: String,
    port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MirrorUrlError {
    #[error("enter the upstream's http:// or https:// URL")]
    Scheme,
    #[error("the URL is too long")]
    TooLong,
    #[error("the URL cannot contain spaces or control characters")]
    Characters,
    #[error("the URL needs a host name")]
    Host,
    #[error("credentials in the URL are not supported; mirror a public repository")]
    Credentials,
    #[error("the port is not valid")]
    Port,
}

impl MirrorUrl {
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The host name or address literal (IPv6 without brackets).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The explicit port, or the scheme's default.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for MirrorUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

impl FromStr for MirrorUrl {
    type Err = MirrorUrlError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let url = input.trim();
        if url.len() > MAX_MIRROR_URL_LEN {
            return Err(MirrorUrlError::TooLong);
        }
        if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(MirrorUrlError::Characters);
        }
        let lower = url.to_ascii_lowercase();
        let (rest, default_port) = if lower.starts_with("https://") {
            (&url[8..], 443)
        } else if lower.starts_with("http://") {
            (&url[7..], 80)
        } else {
            return Err(MirrorUrlError::Scheme);
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return Err(MirrorUrlError::Credentials);
        }
        let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
            let (host, after) = bracketed.split_once(']').ok_or(MirrorUrlError::Host)?;
            (host, after.strip_prefix(':'))
        } else {
            match authority.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            }
        };
        if host.is_empty() || host.contains(['\\', '%']) {
            return Err(MirrorUrlError::Host);
        }
        let port = match port {
            None => default_port,
            Some(port) => port.parse().ok().filter(|p| *p != 0).ok_or(MirrorUrlError::Port)?,
        };
        Ok(Self { url: url.to_owned(), host: host.to_ascii_lowercase(), port })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_validated() {
        let url: MirrorUrl = " https://codeberg.org/forgejo/forgejo.git ".parse().unwrap();
        assert_eq!((url.as_str(), url.host(), url.port()), ("https://codeberg.org/forgejo/forgejo.git", "codeberg.org", 443));
        let url: MirrorUrl = "http://[::1]:8080/x".parse().unwrap();
        assert_eq!((url.host(), url.port()), ("::1", 8080));
        assert_eq!("HTTP://Example.COM".parse::<MirrorUrl>().unwrap().host(), "example.com");

        let cases = [
            ("git://example.com/x", MirrorUrlError::Scheme),
            ("file:///etc", MirrorUrlError::Scheme),
            ("ssh://git@example.com/x", MirrorUrlError::Scheme),
            ("https://user:secret@example.com/x", MirrorUrlError::Credentials),
            ("https:///x", MirrorUrlError::Host),
            ("https://example.com:0/x", MirrorUrlError::Port),
            ("https://example.com:99999/x", MirrorUrlError::Port),
            ("https://exa mple.com", MirrorUrlError::Characters),
        ];
        for (input, error) in cases {
            assert_eq!(input.parse::<MirrorUrl>(), Err(error), "{input}");
        }
        assert_eq!(format!("https://e.com/{}", "a".repeat(2048)).parse::<MirrorUrl>(), Err(MirrorUrlError::TooLong));
    }
}
