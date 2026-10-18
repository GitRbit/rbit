//! `[issues]`: commit references and in-app notifications.
//!
//! ```toml
//! [issues]
//! # Commits scanned for `#N` references and closing keywords per pushed ref update.
//! scan_commit_limit = 250
//! # In-app notifications (the bell in the header and /notifications).
//! notifications = true
//! # Read notifications older than this are pruned daily.
//! notification_retention_days = 90
//! ```
//!
//! Milestone 2 workstream I (issues) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IssueConfig {
    /// Commits scanned per pushed ref update for references and closing keywords.
    pub scan_commit_limit: usize,
    /// Whether in-app notifications are produced and shown.
    pub notifications: bool,
    /// Days a read notification is kept.
    pub notification_retention_days: u32,
}

impl Default for IssueConfig {
    fn default() -> Self {
        Self { scan_commit_limit: 250, notifications: true, notification_retention_days: 90 }
    }
}
