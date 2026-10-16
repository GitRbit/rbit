//! `[operations]`: background maintenance, metrics and backups.
//!
//! Milestone 1 workstream O (operations) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OperationsConfig {
    /// Expose Prometheus metrics at `/metrics`.
    pub metrics: bool,
}
