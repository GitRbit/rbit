//! Exact thread title/body/comment validation and line-ending normalization.

use std::fmt;

/// Longest issue or pull request title, in characters.
pub const MAX_TITLE_CHARS: usize = 256;
/// Longest body or comment, in bytes (after line endings are normalised).
pub const MAX_TEXT_BYTES: usize = 65_536;

/// Why a title, body or comment was refused. Messages are shown to users as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueTextError {
    Empty { kind: &'static str },
    TooManyCharacters { kind: &'static str, max: usize },
    TooLong { kind: &'static str, max: usize },
    ControlCharacter { kind: &'static str },
}

/// An issue or pull request title: trimmed, `1..=256` characters, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueTitle(String);

impl IssueTitle {
    pub fn parse(value: &str) -> Result<Self, IssueTextError> {
        let kind = "title";
        let value = value.trim();
        if value.is_empty() {
            return Err(IssueTextError::Empty { kind });
        }
        if value.chars().count() > MAX_TITLE_CHARS {
            return Err(IssueTextError::TooManyCharacters { kind, max: MAX_TITLE_CHARS });
        }
        if value.chars().any(char::is_control) {
            return Err(IssueTextError::ControlCharacter { kind });
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IssueTitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<IssueTitle> for String {
    fn from(title: IssueTitle) -> Self {
        title.0
    }
}

/// Markdown for issue bodies and comments: at most [`MAX_TEXT_BYTES`], with `\r\n` (and
/// lone `\r`) normalised to `\n`. May be empty for bodies ([`IssueText::body`]), never
/// blank for comments ([`IssueText::comment`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct IssueText(String);

impl IssueText {
    /// An issue or pull request description, possibly empty.
    pub fn body(value: &str) -> Result<Self, IssueTextError> {
        Self::normalise("description", value)
    }

    /// A comment: like a body, but it must contain something other than whitespace.
    pub fn comment(value: &str) -> Result<Self, IssueTextError> {
        let text = Self::normalise("comment", value)?;
        if text.0.trim().is_empty() {
            return Err(IssueTextError::Empty { kind: "comment" });
        }
        Ok(text)
    }

    fn normalise(kind: &'static str, value: &str) -> Result<Self, IssueTextError> {
        let text = value.replace("\r\n", "\n").replace('\r', "\n");
        if text.len() > MAX_TEXT_BYTES {
            return Err(IssueTextError::TooLong { kind, max: MAX_TEXT_BYTES });
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for IssueText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<IssueText> for String {
    fn from(text: IssueText) -> Self {
        text.0
    }
}

impl fmt::Display for IssueTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} must not be empty"),
            Self::TooManyCharacters { kind, max } => write!(formatter, "{kind} must be at most {max} characters"),
            Self::TooLong { kind, max } => write!(formatter, "{kind} must be at most {max} bytes"),
            Self::ControlCharacter { kind } => write!(formatter, "{kind} must not contain control characters"),
        }
    }
}

impl std::error::Error for IssueTextError {}
