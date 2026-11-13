//! The masking pass: secret values out of stored text.
use aho_corasick::{AhoCorasick, MatchKind};

/// Masked text becomes this.
pub const MASK: &str = "***";
/// Shorter values are not masked: they would blank out ordinary words.
pub const MIN_MASKED_CHARS: usize = 4;

/// Replaces every known secret in text with [`MASK`].
#[derive(Debug, Clone)]
pub struct Masker {
    automaton: Option<AhoCorasick>,
}

impl Masker {
    /// Masks every value, and every line of a multi-line value, of at least
    /// [`MIN_MASKED_CHARS`] characters.
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let mut patterns: Vec<&str> = Vec::new();
        for value in values {
            patterns.push(value);
            if value.contains('\n') {
                patterns.extend(value.lines().map(|line| line.trim_end_matches('\r')));
            }
        }
        patterns.retain(|pattern| pattern.chars().count() >= MIN_MASKED_CHARS);
        patterns.sort_unstable();
        patterns.dedup();
        let automaton =
            if patterns.is_empty() { None } else { AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(&patterns).ok() };
        Self { automaton }
    }

    /// Masks nothing.
    pub fn none() -> Self {
        Self { automaton: None }
    }

    /// `text` with every secret replaced, and every task token by its shape: a task's own
    /// token is masked even where its value is no longer known (only its digest is stored,
    /// so a restarted server cannot list it among the secrets).
    pub fn mask(&self, text: &str) -> String {
        let masked = match &self.automaton {
            Some(automaton) if automaton.is_match(text) => automaton.replace_all(text, &vec![MASK; automaton.patterns_len()]),
            _ => text.to_owned(),
        };
        mask_task_tokens(masked)
    }
}

/// A task token: `rbat_` and 43 base64url characters (`rbit_auth::TaskTokenSecret`; the
/// prefix is `rbit_core::actions::TASK_TOKEN_PREFIX`, which `rbit-services` checks it against).
pub const TASK_TOKEN_PREFIX: &str = "rbat_";
const TASK_TOKEN_SECRET_CHARS: usize = 43;

/// `text` with every run of the task token prefix and 43 base64url characters replaced.
fn mask_task_tokens(text: String) -> String {
    if !text.contains(TASK_TOKEN_PREFIX) {
        return text;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find(TASK_TOKEN_PREFIX) {
        let after = &rest[at + TASK_TOKEN_PREFIX.len()..];
        let secret = after.bytes().take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')).count();
        out.push_str(&rest[..at]);
        if secret >= TASK_TOKEN_SECRET_CHARS {
            out.push_str(MASK);
            rest = &after[TASK_TOKEN_SECRET_CHARS..];
        } else {
            out.push_str(TASK_TOKEN_PREFIX);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}
