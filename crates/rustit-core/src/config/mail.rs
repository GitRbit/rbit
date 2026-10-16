//! `[mail]`: outgoing email (password reset, verification, notifications).
//!
//! Milestone 1 workstream E (email & recovery) owns this file.
//!
//! The defaults are safe: mail is disabled until an operator chooses a transport, and an
//! SMTP relay is reached with STARTTLS (refusing to continue without it) unless configured
//! otherwise.
//!
//! ```toml
//! [mail]
//! transport = "smtp"            # "disabled" (default), "log" or "smtp"
//! from_address = "git@example.com"
//! from_name = "Example Git"     # blank: the site name
//! smtp_host = "smtp.example.com"
//! smtp_port = 587               # blank: 587 for starttls/none, 465 for tls
//! smtp_security = "starttls"    # "starttls" (default), "tls" or "none"
//! smtp_username = "git@example.com"
//! smtp_password = "…"
//! ```

use std::fmt;

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

/// How the connection to the SMTP relay is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// Plain connection upgraded with `STARTTLS`; the upgrade is required (port 587).
    #[default]
    Starttls,
    /// TLS from the first byte, also called SMTPS or implicit TLS (port 465).
    Tls,
    /// No encryption at all. Only for a relay on the same host or a trusted network.
    None,
}

impl SmtpSecurity {
    /// The conventional port for this kind of connection.
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Starttls | Self::None => 587,
            Self::Tls => 465,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    pub transport: MailTransport,
    /// The `From:` address of every message.
    pub from_address: String,
    /// The display name in `From:`. Blank means the site name.
    pub from_name: String,
    /// Host name of the SMTP relay (`transport = "smtp"`).
    pub smtp_host: String,
    /// Port of the SMTP relay. `None` picks the conventional port for `smtp_security`.
    pub smtp_port: Option<u16>,
    pub smtp_security: SmtpSecurity,
    /// Credentials for the relay. Blank means no authentication.
    pub smtp_username: String,
    pub smtp_password: String,
    /// Give up on a message that has not been delivered after this many seconds.
    pub timeout_seconds: u64,
}

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            transport: MailTransport::Disabled,
            from_address: "rustit@localhost".to_owned(),
            from_name: String::new(),
            smtp_host: String::new(),
            smtp_port: None,
            smtp_security: SmtpSecurity::Starttls,
            smtp_username: String::new(),
            smtp_password: String::new(),
            timeout_seconds: 30,
        }
    }
}

impl MailConfig {
    /// Whether flows that depend on email (password reset, verification) are available.
    pub fn is_enabled(&self) -> bool {
        self.transport != MailTransport::Disabled
    }

    /// The SMTP port actually used.
    pub fn smtp_port(&self) -> u16 {
        self.smtp_port.unwrap_or_else(|| self.smtp_security.default_port())
    }

    /// Whether the relay is reached over TLS from the first byte (`smtp_security = "tls"`).
    pub fn smtp_implicit_tls(&self) -> bool {
        self.smtp_security == SmtpSecurity::Tls
    }

    /// Whether the relay is reached without any encryption (`smtp_security = "none"`).
    pub fn smtp_unencrypted(&self) -> bool {
        self.smtp_security == SmtpSecurity::None
    }

    /// Credentials for the relay, if any are configured.
    pub fn smtp_credentials(&self) -> Option<(&str, &str)> {
        (!self.smtp_username.is_empty()).then_some((self.smtp_username.as_str(), self.smtp_password.as_str()))
    }
}

/// The password never appears in logs, not even debug output of the whole configuration.
impl fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailConfig")
            .field("transport", &self.transport)
            .field("from_address", &self.from_address)
            .field("from_name", &self.from_name)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_security", &self.smtp_security)
            .field("smtp_username", &self.smtp_username)
            .field("smtp_password", &if self.smtp_password.is_empty() { "" } else { "<redacted>" })
            .field("timeout_seconds", &self.timeout_seconds)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let config = MailConfig::default();
        assert!(!config.is_enabled());
        assert_eq!(config.smtp_security, SmtpSecurity::Starttls);
        assert_eq!(config.smtp_port(), 587);
        assert_eq!(config.smtp_credentials(), None);
    }

    #[test]
    fn port_follows_security_unless_set() {
        let mut config = MailConfig { smtp_security: SmtpSecurity::Tls, ..MailConfig::default() };
        assert_eq!(config.smtp_port(), 465);
        config.smtp_port = Some(2525);
        assert_eq!(config.smtp_port(), 2525);
    }

    #[test]
    fn debug_output_redacts_the_password() {
        let config = MailConfig { smtp_username: "git".to_owned(), smtp_password: "hunter2".to_owned(), ..MailConfig::default() };
        let debug = format!("{config:?}");
        assert!(!debug.contains("hunter2"));
        assert!(debug.contains("<redacted>"));
        assert_eq!(config.smtp_credentials(), Some(("git", "hunter2")));
    }
}
