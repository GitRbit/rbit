//! `[notifications]`: email notifications on top of the in-app inbox.
//!
//! ```toml
//! [notifications]
//! # Also send notifications by email (needs a [mail] transport and a verified address).
//! email = true
//! # What users who never chose get: "off", "participating" (default) or "all".
//! default_email = "participating"
//! ```
//!
//! Milestone 3 workstream N (email notifications) owns this file. The inbox itself is
//! configured in `[issues]` (`notifications`, `notification_retention_days`); with the
//! inbox off no email is sent either.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Which notifications a user also receives by email.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmailPreference {
    /// No notification email at all.
    Off,
    /// Threads the user takes part in: opened, commented on, reviewed, was mentioned in,
    /// assigned to or asked to review. Threads only subscribed to by hand stay in the inbox.
    #[default]
    Participating,
    /// Everything that reaches the inbox, including threads subscribed to by hand.
    All,
}

impl EmailPreference {
    pub const ALL: [Self; 3] = [Self::Off, Self::Participating, Self::All];

    /// The stable identifier stored in the database and used in forms.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Participating => "participating",
            Self::All => "all",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preference| preference.as_str() == value)
    }
}

impl fmt::Display for EmailPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    /// Whether notifications are also sent by email.
    pub email: bool,
    /// The preference of users who never chose one.
    pub default_email: EmailPreference,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { email: true, default_email: EmailPreference::Participating }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip() {
        for preference in EmailPreference::ALL {
            assert_eq!(EmailPreference::parse(preference.as_str()), Some(preference));
        }
        assert_eq!(EmailPreference::parse("never"), None);
        assert_eq!(EmailPreference::default(), EmailPreference::Participating);
    }

    #[test]
    fn parses_the_section() {
        let config: NotificationConfig = serde_json::from_str(r#"{"email": false, "default_email": "all"}"#).unwrap();
        assert!(!config.email);
        assert_eq!(config.default_email, EmailPreference::All);
        let config: NotificationConfig = serde_json::from_str("{}").unwrap();
        assert!(config.email);
        assert_eq!(config.default_email, EmailPreference::Participating);
    }
}
