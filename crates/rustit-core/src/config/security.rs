//! `[security]`: sessions, passwords and abuse protection.
//!
//! Milestone 1 workstream S (security) owns this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// How long a session stays valid without activity, in days.
    pub session_lifetime_days: u32,
    /// Minimum password length accepted at sign-up.
    pub min_password_length: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self { session_lifetime_days: 30, min_password_length: 10 }
    }
}
