//! Email syntax compatibility; mailbox control is verified by the application.

use std::{fmt, str::FromStr};

use rbit_contracts::NameError;
use serde::{Deserialize, Serialize};

/// A syntactically plausible email address. Deliverability is verified separately by
/// sending a confirmation link; this type only rules out obvious garbage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Email(String);

impl Email {
    pub const MAX_LEN: usize = 254;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Email {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let plausible = value.len() <= Self::MAX_LEN
            && !value.chars().any(char::is_whitespace)
            && value.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
            });
        if plausible { Ok(Self(value.to_owned())) } else { Err(NameError::InvalidEmail) }
    }
}

impl TryFrom<String> for Email {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Email> for String {
    fn from(email: Email) -> Self {
        email.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
