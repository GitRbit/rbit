//! `[mirrors]`: pull mirrors that fetch from an upstream repository on a schedule.
//!
//! ```toml
//! [mirrors]
//! enabled = true
//! default_interval_minutes = 480
//! min_interval_minutes = 10
//! fetch_timeout_seconds = 900
//! # Upstreams on loopback, private, link-local and unique-local addresses are refused
//! # unless this is set.
//! allow_private_addresses = false
//! ```
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MirrorConfig {
    /// Allow creating mirrors and run scheduled fetches.
    pub enabled: bool,
    /// Fetch interval for new mirrors.
    pub default_interval_minutes: u64,
    /// The shortest interval a mirror may be configured with.
    pub min_interval_minutes: u64,
    /// Upper bound for one fetch.
    pub fetch_timeout_seconds: u64,
    /// Allow upstream URLs that resolve to loopback, private, link-local or unique-local addresses.
    pub allow_private_addresses: bool,
}

impl Default for MirrorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_interval_minutes: 8 * 60,
            min_interval_minutes: 10,
            fetch_timeout_seconds: 15 * 60,
            allow_private_addresses: false,
        }
    }
}
