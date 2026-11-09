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

    /// `text` with every secret replaced.
    pub fn mask(&self, text: &str) -> String {
        match &self.automaton {
            Some(automaton) if automaton.is_match(text) => automaton.replace_all(text, &vec![MASK; automaton.patterns_len()]),
            _ => text.to_owned(),
        }
    }
}
