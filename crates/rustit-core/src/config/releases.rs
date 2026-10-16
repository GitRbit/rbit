//! `[releases]`: release notes and attached files.
//!
//! ```toml
//! [releases]
//! # Largest file that may be attached to a release, in bytes.
//! max_asset_bytes = 536870912
//! max_assets_per_release = 50
//! # Upper bound for one upload; uploads bypass the 30-second page timeout.
//! upload_timeout_seconds = 3600
//! ```
//!
//! Files are stored under `<data_dir>/attachments` (see `StorageConfig::attachments_dir`).
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseConfig {
    /// Largest attached file, in bytes.
    pub max_asset_bytes: u64,
    /// Attached files per release.
    pub max_assets_per_release: u32,
    /// Upper bound, in seconds, for one upload.
    pub upload_timeout_seconds: u64,
}

impl Default for ReleaseConfig {
    fn default() -> Self {
        Self { max_asset_bytes: 512 * 1024 * 1024, max_assets_per_release: 50, upload_timeout_seconds: 60 * 60 }
    }
}
