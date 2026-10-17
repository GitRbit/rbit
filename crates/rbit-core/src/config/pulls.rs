//! `[pulls]`: merging pull requests and computing their mergeability.
//!
//! ```toml
//! [pulls]
//! # Upper bound for one merge or trial merge (all its git work), in seconds.
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
//! A merge never clones or checks out anything. It runs in a scratch bare repository under
//! `<data_dir>/tmp/merges/<random>` that borrows the base repository's objects, computes
//! the result with `git merge-tree --write-tree` and `git commit-tree` (no work tree, no
//! hooks, no repository configuration), then the base repository fetches the result and
//! moves the branch with `update-ref <new> <expected old>`, the atomic lease: a branch
//! that moved meanwhile is refused as `BaseMoved` and the merge is retried once. The whole
//! merge is wrapped in `PushService`, so push events fire as for any push (see
//! `rbit_git::merge` and `rbit_services::pulls::merging`). Comparison overlays also work
//! under `<data_dir>/tmp` (see `StorageConfig::tmp_dir`).
//!
//! Milestone 2 workstream P (pull requests) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PullConfig {
    /// Upper bound, in seconds, for one merge or trial merge: all of its git work, from
    /// creating the scratch repository to the base repository's fetch and ref update.
    pub merge_timeout_seconds: u64,
    /// Merges running at once across the instance (always one per base repository).
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
