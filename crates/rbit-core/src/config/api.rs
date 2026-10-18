//! `[api]`: the versioned REST API under `/api/v1`.
//!
//! ```toml
//! [api]
//! enabled = true
//! # Items per page when a request does not ask (`?per_page=`), and the most it may ask for.
//! default_page_size = 30
//! max_page_size = 100
//! ```
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiConfig {
    /// Serve `/api/v1`. When off, every API path answers 404.
    pub enabled: bool,
    /// Items per page when the request does not say.
    pub default_page_size: u32,
    /// Upper bound for `?per_page=`.
    pub max_page_size: u32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self { enabled: true, default_page_size: 30, max_page_size: 100 }
    }
}
