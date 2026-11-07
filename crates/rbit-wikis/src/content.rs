//! Bounded source text, not sanitized/rendered HTML.
use std::fmt;

pub const MAX_WIKI_CONTENT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiContentError {
    TooLarge,
    Nul,
}

/// Original UTF-8 Markdown. Rendering must still escape/sanitize untrusted content.
#[derive(PartialEq, Eq)]
pub struct WikiContent(String);

impl WikiContent {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WikiContent {
    type Error = WikiContentError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > MAX_WIKI_CONTENT_BYTES {
            return Err(WikiContentError::TooLarge);
        }
        if value.contains('\0') {
            return Err(WikiContentError::Nul);
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for WikiContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("WikiContent").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}
