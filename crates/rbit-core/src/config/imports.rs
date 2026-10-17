//! `[imports]`: importing repositories from GitHub and Gitea/Forgejo, and
//! `[push_mirrors]`: pushing repositories to other remotes.
//!
//! ```toml
//! [imports]
//! enabled = true
//! # Sources (API, Git, release files, LFS objects) on loopback, private, link-local and
//! # unique-local addresses are refused unless this is set.
//! allow_private_addresses = false
//! # Imports running at the same time, instance-wide; more wait in line.
//! max_concurrent = 2
//! # Upper bound for copying the Git repository.
//! git_timeout_seconds = 3600
//! # Upper bound for one API request (and for one release file or LFS object download,
//! # multiplied by 10).
//! api_timeout_seconds = 60
//! # When the source's API rate limit runs out, wait at most this long for it to reset;
//! # a longer wait fails the import.
//! rate_limit_max_wait_seconds = 900
//! # Attribute imported issues and comments to local accounts: "none" (default: the
//! # importing user writes everything, with "originally by @login on GitHub"), "email"
//! # (an account with the same verified email address) or "email_or_username" (also an
//! # account with the same user name).
//! author_mapping = "none"
//!
//! [push_mirrors]
//! enabled = true
//! # Push every mirror at least this often (besides after pushes, when enabled).
//! interval_minutes = 480
//! # Wait this long after a push before pushing to mirrors, so a burst of pushes is sent once.
//! debounce_seconds = 10
//! push_timeout_seconds = 900
//! allow_private_addresses = false
//! ```
//!
//! `[push_mirrors] allow_local_urls` (default `false`) additionally admits `file://`
//! remotes. It exists for tests: on a real instance it would let repository
//! administrators write into any directory the server can.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    /// Offer importing on the "New repository" page.
    pub enabled: bool,
    /// Allow sources that resolve to loopback, private, link-local or unique-local addresses.
    pub allow_private_addresses: bool,
    /// Imports running at the same time.
    pub max_concurrent: usize,
    /// Upper bound for the Git copy.
    pub git_timeout_seconds: u64,
    /// Upper bound for one API request.
    pub api_timeout_seconds: u64,
    /// Longest wait for the source's rate limit to reset.
    pub rate_limit_max_wait_seconds: u64,
    /// How authors on the source become local accounts.
    pub author_mapping: AuthorMapping,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_private_addresses: false,
            max_concurrent: 2,
            git_timeout_seconds: 60 * 60,
            api_timeout_seconds: 60,
            rate_limit_max_wait_seconds: 15 * 60,
            author_mapping: AuthorMapping::None,
        }
    }
}

/// How authors on the import source are matched with local accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorMapping {
    /// Nobody is matched: the importing user writes everything, naming the original author.
    #[default]
    None,
    /// An account whose verified email address is the author's public one.
    Email,
    /// As `email`, or else an account with the author's user name.
    EmailOrUsername,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PushMirrorConfig {
    /// Allow configuring push mirrors and push to them.
    pub enabled: bool,
    /// Push every mirror at least this often.
    pub interval_minutes: u64,
    /// Delay after a push before mirrors are pushed.
    pub debounce_seconds: u64,
    /// Upper bound for one `git push`.
    pub push_timeout_seconds: u64,
    /// Allow remotes that resolve to loopback, private, link-local or unique-local addresses.
    pub allow_private_addresses: bool,
    /// Admit `file://` remotes (tests only; see the module docs).
    pub allow_local_urls: bool,
}

impl Default for PushMirrorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 8 * 60,
            debounce_seconds: 10,
            push_timeout_seconds: 15 * 60,
            allow_private_addresses: false,
            allow_local_urls: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let imports = ImportConfig::default();
        assert!(imports.enabled && !imports.allow_private_addresses);
        assert_eq!(imports.author_mapping, AuthorMapping::None);
        let mirrors = PushMirrorConfig::default();
        assert!(mirrors.enabled && !mirrors.allow_private_addresses && !mirrors.allow_local_urls);
    }
}
