//! Branch patterns: an exact branch name or a glob (`*` within one path segment, `**`
//! across segments, `?` one character other than `/`). On Windows and macOS, case
//! aliases match, because repositories commonly live on case-insensitive filesystems.

use std::{fmt, str::FromStr};

use super::reasons;

/// Longest pattern, in bytes.
pub const MAX_PATTERN_LEN: usize = 255;

/// Why a pattern was rejected. The message is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("enter a branch name or pattern")]
    Empty,
    #[error("the pattern must be at most {MAX_PATTERN_LEN} bytes")]
    TooLong,
    #[error("the pattern may not contain {0:?}")]
    Character(char),
    #[error("the pattern may not contain spaces or control characters")]
    Whitespace,
    #[error("the pattern may not contain {0:?}")]
    Sequence(&'static str),
    #[error("the pattern may not start or end with \"/\" or end with \".\"")]
    Edge,
    #[error("no part of the pattern may start with \".\" or end with \".lock\"")]
    Segment,
    #[error("use * or ** (not three or more stars in a row)")]
    Stars,
}

/// A branch name or glob a rule applies to (without `refs/heads/`). Validated: at most
/// [`MAX_PATTERN_LEN`] bytes of the characters a branch name may contain, plus `*` and `?`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BranchPattern(String);

/// One element of a parsed pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(char),
    /// `?`: one character other than `/`.
    One,
    /// `*`: any run of characters other than `/`.
    Star,
    /// `**`: any run of characters, `/` included. `**/` also matches nothing at all, so
    /// `**/main` matches `main` too.
    Globstar,
}

impl BranchPattern {
    pub fn parse(value: &str) -> Result<Self, PatternError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(PatternError::Empty);
        }
        if value.len() > MAX_PATTERN_LEN {
            return Err(PatternError::TooLong);
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(PatternError::Whitespace);
        }
        if let Some(c) = value.chars().find(|c| matches!(c, '~' | '^' | ':' | '[' | '\\')) {
            return Err(PatternError::Character(c));
        }
        for sequence in ["..", "@{", "//"] {
            if value.contains(sequence) {
                return Err(PatternError::Sequence(sequence));
            }
        }
        if value == "@" {
            return Err(PatternError::Character('@'));
        }
        if value.starts_with('/') || value.ends_with('/') || value.ends_with('.') {
            return Err(PatternError::Edge);
        }
        if value.split('/').any(|segment| segment.starts_with('.') || segment.strip_suffix(".lock").is_some()) {
            return Err(PatternError::Segment);
        }
        if value.contains("***") {
            return Err(PatternError::Stars);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a glob rather than one exact branch name.
    pub fn is_glob(&self) -> bool {
        self.0.contains(['*', '?'])
    }

    fn tokens(&self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(self.0.len());
        let mut chars = self.0.chars().peekable();
        while let Some(c) = chars.next() {
            tokens.push(match c {
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                    Token::Globstar
                }
                '*' => Token::Star,
                '?' => Token::One,
                other => Token::Literal(other),
            });
        }
        tokens
    }

    /// Whether `branch` (a name without `refs/heads/`) matches. Case aliases match on
    /// Windows and macOS so filesystem aliases cannot escape protection.
    pub fn matches(&self, branch: &str) -> bool {
        if !self.is_glob() {
            return ref_names_equal(&self.0, branch);
        }
        let tokens = self.tokens();
        let text: Vec<char> = branch.chars().collect();
        // matched[t][s]: tokens[t..] match text[s..]. Filled backwards; O(tokens × text).
        let width = text.len() + 1;
        let mut matched = vec![false; (tokens.len() + 1) * width];
        matched[tokens.len() * width + text.len()] = true;
        for t in (0..tokens.len()).rev() {
            for s in (0..=text.len()).rev() {
                let next = |t: usize, s: usize| matched[t * width + s];
                let here = match tokens[t] {
                    Token::Literal(c) => s < text.len() && ref_char_equal(text[s], c) && next(t + 1, s + 1),
                    Token::One => s < text.len() && text[s] != '/' && next(t + 1, s + 1),
                    Token::Star => next(t + 1, s) || (s < text.len() && text[s] != '/' && next(t, s + 1)),
                    Token::Globstar => {
                        next(t + 1, s)
                            || (s < text.len() && next(t, s + 1))
                            || (tokens.get(t + 1) == Some(&Token::Literal('/')) && next(t + 2, s))
                    }
                };
                matched[t * width + s] = here;
            }
        }
        matched[0]
    }
}

/// Whether two ref names can identify the same filesystem entry. Matching is
/// conservatively case-insensitive on Windows and macOS, including Unicode case
/// mappings; the original spelling is still used for every Git operation.
pub(crate) fn ref_names_equal(left: &str, right: &str) -> bool {
    if !cfg!(any(windows, target_os = "macos")) {
        return left == right;
    }
    let mut right = right.chars();
    left.chars().all(|left| right.next().is_some_and(|right| ref_char_equal(left, right))) && right.next().is_none()
}

fn ref_char_equal(left: char, right: char) -> bool {
    left == right
        || (cfg!(any(windows, target_os = "macos"))
            && (left.to_lowercase().eq(right.to_lowercase()) || left.to_uppercase().eq(right.to_uppercase())))
}

pub(crate) fn namespace_refusal(name: &str) -> Option<&'static str> {
    let (refs, rest) = name.split_once('/')?;
    let (namespace, _) = rest.split_once('/')?;
    if !ref_names_equal(refs, "refs") {
        return None;
    }
    if ref_names_equal(namespace, "pull") {
        return Some(reasons::PULL_REFS);
    }
    // Git's receive.denyNonFastForwards only recognises the exact refs/heads/
    // spelling, even when its backing filesystem treats another spelling as equal.
    if ref_names_equal(namespace, "heads") && (refs != "refs" || namespace != "heads") {
        return Some(reasons::NAMESPACE_CASE);
    }
    None
}

impl FromStr for BranchPattern {
    type Err = PatternError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for BranchPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
