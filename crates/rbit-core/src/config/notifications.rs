//! `[notifications]`: email notifications on top of the in-app inbox.
//!
//! ```toml
//! [notifications]
//! # Also send notifications by email (needs a [mail] transport and a verified address).
//! email = true
//! # What users who never chose get: "off", "participating" (default) or "all".
//! default_email = "participating"
//! # The hour (0-23, UTC) daily and weekly digests are sent at, unless a user chooses
//! # another. Administrators can change it at runtime.
//! digest_hour = 8
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

/// When a user's notification emails are sent: one by one as they happen, or collected
/// into one digest a day or a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmailDelivery {
    /// One email per notification, as it happens.
    #[default]
    Immediate,
    /// One digest a day, at the digest hour.
    Daily,
    /// One digest a week, on Mondays at the digest hour.
    Weekly,
}

impl EmailDelivery {
    pub const ALL: [Self; 3] = [Self::Immediate, Self::Daily, Self::Weekly];

    /// The stable identifier stored in the database and used in forms.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|delivery| delivery.as_str() == value)
    }

    /// Whether notifications wait for a digest.
    pub const fn is_digest(self) -> bool {
        !matches!(self, Self::Immediate)
    }
}

impl fmt::Display for EmailDelivery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The last valid digest hour (hours are 0-23, UTC).
pub const MAX_DIGEST_HOUR: u8 = 23;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationConfig {
    /// Whether notifications are also sent by email.
    pub email: bool,
    /// The preference of users who never chose one.
    pub default_email: EmailPreference,
    /// The hour (UTC) digests are sent at for users who did not choose one; the instance
    /// settings can override it at runtime.
    #[serde(deserialize_with = "digest_hour")]
    pub digest_hour: u8,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { email: true, default_email: EmailPreference::Participating, digest_hour: 8 }
    }
}

fn digest_hour<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let hour = u8::deserialize(deserializer)?;
    if hour > MAX_DIGEST_HOUR {
        return Err(serde::de::Error::custom(format!("digest_hour must be between 0 and {MAX_DIGEST_HOUR}, not {hour}")));
    }
    Ok(hour)
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
        for delivery in EmailDelivery::ALL {
            assert_eq!(EmailDelivery::parse(delivery.as_str()), Some(delivery));
        }
        assert_eq!(EmailDelivery::parse("hourly"), None);
        assert_eq!(EmailDelivery::default(), EmailDelivery::Immediate);
        assert!(!EmailDelivery::Immediate.is_digest() && EmailDelivery::Daily.is_digest() && EmailDelivery::Weekly.is_digest());
    }

    #[test]
    fn parses_the_section() {
        let config: NotificationConfig = serde_json::from_str(r#"{"email": false, "default_email": "all"}"#).unwrap();
        assert!(!config.email);
        assert_eq!(config.default_email, EmailPreference::All);
        let config: NotificationConfig = serde_json::from_str("{}").unwrap();
        assert!(config.email);
        assert_eq!(config.default_email, EmailPreference::Participating);
        assert_eq!(config.digest_hour, 8);
        let config: NotificationConfig = serde_json::from_str(r#"{"digest_hour": 23}"#).unwrap();
        assert_eq!(config.digest_hour, 23);
        assert!(serde_json::from_str::<NotificationConfig>(r#"{"digest_hour": 24}"#).is_err());
    }
}
