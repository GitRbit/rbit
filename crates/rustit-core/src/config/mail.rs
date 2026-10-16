//! `[mail]`: outgoing email (password reset, verification, notifications).
//!
//! Milestone 1 workstream E (email & recovery) owns this file.

use serde::{Deserialize, Serialize};

/// How outgoing mail is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailTransport {
    /// Mail is not sent; flows that need it explain that email is unavailable.
    #[default]
    Disabled,
    /// Messages are written to the log instead of being sent (development).
    Log,
    /// Messages are delivered through an SMTP relay.
    Smtp,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    pub transport: MailTransport,
}

impl MailConfig {
    /// Whether flows that depend on email (password reset, verification) are available.
    pub fn is_enabled(&self) -> bool {
        self.transport != MailTransport::Disabled
    }
}
