//! Canonical per-repository issue/pull number with unchanged serde and name errors.

use rbit_contracts::NameError;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Per-repository number of an issue or pull request, always `> 0`. Parsed from URL
/// segments (`"12"`) with [`IssueNumber::parse`]; displayed as `#12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct IssueNumber(u32);

impl IssueNumber {
    const KIND: &'static str = "issue number";

    /// Accepts only canonical decimal numbers: digits, no sign, no leading zero, `1..=u32::MAX`.
    /// Anything else (which a URL handler answers with 404) is an error.
    pub fn parse(value: &str) -> Result<Self, NameError> {
        let kind = Self::KIND;
        if value.is_empty() {
            return Err(NameError::Empty { kind });
        }
        if value.len() > 10 {
            return Err(NameError::TooLong { kind, max: 10 });
        }
        if !value.bytes().all(|b| b.is_ascii_digit()) || value.starts_with('0') {
            return Err(NameError::NotANumber { kind });
        }
        value.parse::<u32>().map_err(|_| NameError::NotANumber { kind }).and_then(Self::try_from)
    }

    /// `None` for zero.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for IssueNumber {
    type Error = NameError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(NameError::NotANumber { kind: Self::KIND })
    }
}

impl TryFrom<i64> for IssueNumber {
    type Error = NameError;

    /// For database rows.
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value).map_err(|_| NameError::NotANumber { kind: Self::KIND }).and_then(Self::try_from)
    }
}

impl From<IssueNumber> for u32 {
    fn from(number: IssueNumber) -> Self {
        number.0
    }
}

impl FromStr for IssueNumber {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// `#12`. Use [`IssueNumber::get`] for URLs.
impl fmt::Display for IssueNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}
