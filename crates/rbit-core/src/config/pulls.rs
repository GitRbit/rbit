//! `[pulls]`: merging pull requests and computing their mergeability.
//!
//! ```toml
//! [pulls]
//! # Upper bound for one merge (clone, merge, push back), in seconds.
//! merge_timeout_seconds = 120
//! # Merges running at once across the instance (always one per repository).
//! max_concurrent_merges = 2
//! # Trial merges computing mergeability at once.
//! mergeability_concurrency = 2
//! # Commits listed on a pull request's Commits tab.
//! max_commits = 250
//! # Domain of the addresses in commits Rbit makes; empty = "noreply.<host of server.public_url>".
//! noreply_domain = ""
//! ```
//!
//! Merges and overlays work under `<data_dir>/tmp` (see `StorageConfig::tmp_dir`).
//!
//! Milestone 2 workstream P (pull requests) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PullConfig {
    /// Upper bound, in seconds, for one merge.
    pub merge_timeout_seconds: u64,
    /// Merges running at once across the instance.
    pub max_concurrent_merges: usize,
    /// Trial merges for mergeability running at once.
    pub mergeability_concurrency: usize,
    /// Commits shown per pull request.
    pub max_commits: usize,
    /// Domain of the noreply addresses in Rbit-made commits; empty means
    /// `noreply.<host of server.public_url>`.
    pub noreply_domain: String,
}

impl Default for PullConfig {
    fn default() -> Self {
        Self {
            merge_timeout_seconds: 120,
            max_concurrent_merges: 2,
            mergeability_concurrency: 2,
            max_commits: 250,
            noreply_domain: String::new(),
        }
    }
}
