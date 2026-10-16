//! `[webhooks]`: outgoing HTTP deliveries for repository and organisation events.
//!
//! ```toml
//! [webhooks]
//! enabled = true
//! # Deliveries to loopback, private, link-local and unique-local addresses are refused
//! # unless this is set (they would let repository admins probe the internal network).
//! allow_private_addresses = false
//! timeout_seconds = 10
//! max_attempts = 5
//! concurrency = 4
//! delivery_retention_days = 14
//! ```
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookConfig {
    /// Deliver webhooks at all. When off, hooks can still be configured but nothing is sent.
    pub enabled: bool,
    /// Allow targets that resolve to loopback, private, link-local or unique-local addresses.
    pub allow_private_addresses: bool,
    /// Upper bound for one delivery attempt (connect, send, read the response status).
    pub timeout_seconds: u64,
    /// Attempts per delivery, including the first, before it is marked failed.
    pub max_attempts: u32,
    /// Deliveries in flight at once across the instance.
    pub concurrency: usize,
    /// Days the delivery log keeps requests and responses.
    pub delivery_retention_days: u32,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_private_addresses: false,
            timeout_seconds: 10,
            max_attempts: 5,
            concurrency: 4,
            delivery_retention_days: 14,
        }
    }
}
