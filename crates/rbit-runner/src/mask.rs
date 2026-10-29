//! Masking: secrets become `***` before a log line or an output leaves the runner
//! (§12.5.4 "Masking", §11.10).
//!
//! A task's masker starts with its secrets and its task token and grows with every
//! `::add-mask::`. For each value it masks:
//!
//! * the value itself;
//! * each line of a multi-line value that is at least [`MIN_LINE_CHARS`] long, since a
//!   script printing a key file line by line must not reveal it either;
//! * the base64 encoding of each of those, in all three alignments, without the characters
//!   that depend on the bytes around the value; these fragments are kept only when they are
//!   at least [`MIN_BASE64_CHARS`] long, so they cannot match ordinary words;
//! * the other forms GitHub's runner masks ([`escaped_forms`]): URL-encoded (as a URI
//!   component, and as a form value with `+` for spaces), JSON- and XML-escaped, quoted
//!   for an expression (`''`) or a command line (`\"`), and without surrounding double
//!   quotes.
//!
//! All patterns go into one Aho-Corasick automaton per task, matched leftmost-longest, so
//! overlapping values are masked as a whole. The automaton is rebuilt when a value is
//! added, which happens rarely and only between lines. The server masks once more (§11.10).

use std::{
    borrow::Cow,
    collections::BTreeSet,
    sync::{Mutex, PoisonError},
};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use base64::Engine as _;

/// What a masked value becomes.
pub const MASK: &str = "***";
/// Shortest line of a multi-line value that is masked on its own.
pub const MIN_LINE_CHARS: usize = 4;
/// Shortest base64 fragment that is masked.
pub const MIN_BASE64_CHARS: usize = 6;
/// Most values a task may mask; `::add-mask::` beyond it is refused.
pub const MAX_VALUES: usize = 4096;

/// The masker of one task.
#[derive(Debug, Default)]
pub struct Masker {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    values: BTreeSet<String>,
    patterns: BTreeSet<String>,
    automaton: Option<AhoCorasick>,
}

impl Masker {
    #[cfg(test)]
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let masker = Self::default();
        masker.add_all(values);
        masker
    }

    /// Masks `value` from now on. `false` when the task already masks too many values.
    pub fn add(&self, value: &str) -> bool {
        self.add_all([value])
    }

    fn add_all<'a>(&self, values: impl IntoIterator<Item = &'a str>) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut changed = false;
        for value in values {
            if value.trim().is_empty() || state.values.contains(value) {
                continue;
            }
            if state.values.len() >= MAX_VALUES {
                return false;
            }
            state.values.insert(value.to_owned());
            for pattern in patterns(value) {
                changed |= state.patterns.insert(pattern);
            }
        }
        if changed {
            state.automaton = AhoCorasickBuilder::new().match_kind(MatchKind::LeftmostLongest).build(state.patterns.iter()).ok();
        }
        true
    }

    /// `text` with every masked value replaced by [`MASK`].
    pub fn mask<'t>(&self, text: &'t str) -> Cow<'t, str> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match &state.automaton {
            Some(automaton) if automaton.is_match(text) => {
                let mut masked = String::with_capacity(text.len());
                automaton.replace_all_with(text, &mut masked, |_, _, out| {
                    out.push_str(MASK);
                    true
                });
                Cow::Owned(masked)
            }
            _ => Cow::Borrowed(text),
        }
    }
}

/// Everything to mask for `value`: itself, its long lines, and their base64 forms.
fn patterns(value: &str) -> Vec<String> {
    let mut plain = vec![value.to_owned()];
    if value.contains('\n') {
        plain.extend(
            value
                .split('\n')
                .map(|line| line.trim_end_matches('\r'))
                .filter(|line| line.trim().chars().count() >= MIN_LINE_CHARS)
                .map(str::to_owned),
        );
    }
    let mut all = plain.clone();
    for text in &plain {
        all.extend(base64_forms(text.as_bytes()));
        all.extend(escaped_forms(text).into_iter().filter(|form| form != text));
    }
    all
}

/// `text` as GitHub's runner also masks it: percent-encoded as a URI component (RFC 3986
/// unreserved characters kept) and as a form value (`+` for a space), escaped for JSON, XML,
/// an expression's single-quoted string and a double-quoted command-line argument, and with
/// one pair of surrounding double quotes trimmed.
fn escaped_forms(text: &str) -> Vec<String> {
    let percent = |space_plus: bool| {
        let mut out = String::with_capacity(text.len() * 3);
        for byte in text.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(char::from(byte)),
                b' ' if space_plus => out.push('+'),
                _ => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "%{byte:02X}");
                }
            }
        }
        out
    };
    let json = serde_json::to_string(text).map(|quoted| quoted[1..quoted.len() - 1].to_owned()).unwrap_or_default();
    let xml = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;");
    let mut forms = vec![percent(false), percent(true), json, xml, text.replace('\'', "''"), text.replace('"', "\\\"")];
    if let Some(inner) = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).filter(|inner| !inner.is_empty()) {
        forms.push(inner.to_owned());
    }
    forms.retain(|form| form.trim().chars().count() >= MIN_LINE_CHARS);
    forms
}

/// The base64 of `bytes` as it appears inside a longer encoded text, for each of the three
/// positions `bytes` can start at relative to a 3-byte group.
fn base64_forms(bytes: &[u8]) -> Vec<String> {
    let mut forms = Vec::new();
    for offset in 0..3usize {
        let mut padded = vec![0u8; offset];
        padded.extend_from_slice(bytes);
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&padded);
        // Characters that carry bits of the padding bytes, and a last character that
        // carries bits of whatever follows, are not part of the value's own encoding.
        let skip = (offset * 8).div_ceil(6);
        let drop_last = usize::from(padded.len() % 3 != 0);
        if encoded.len() > skip + drop_last {
            let form = &encoded[skip..encoded.len() - drop_last];
            if form.len() >= MIN_BASE64_CHARS {
                forms.push(form.to_owned());
            }
        }
    }
    forms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn values_lines_and_tokens_are_masked() {
        let masker = Masker::new(["s3cr3t-value", "rbat_TOKEN", "-----BEGIN KEY-----\nAAAAbbbbCCCC\nxy\n-----END KEY-----"]);
        assert_eq!(masker.mask("echo s3cr3t-value done"), "echo *** done");
        assert_eq!(masker.mask("Authorization: rbat_TOKEN"), "Authorization: ***");
        assert_eq!(masker.mask("line AAAAbbbbCCCC"), "line ***");
        assert_eq!(masker.mask("xy stays"), "xy stays", "short lines are not masked on their own");
        assert!(matches!(masker.mask("nothing here"), Cow::Borrowed(_)));
        // Where two values overlap, the leftmost wins.
        let overlapping = Masker::new(["abcdef", "cdefgh"]);
        assert_eq!(overlapping.mask("xabcdefghx"), "x***ghx");
    }

    #[test]
    fn escaped_forms_are_masked_as_on_github() {
        let masker = Masker::new(["p@ss w/rd&more", "pa\"ss\\w0rd-42", "it's <ok>"]);
        for printed in [
            "p%40ss%20w%2Frd%26more",
            "p%40ss+w%2Frd%26more",
            "{\"password\": \"pa\\\"ss\\\\w0rd-42\"}",
            "pa\\\"ss\\w0rd-42",
            "it&apos;s &lt;ok&gt;",
            "'it''s <ok>'",
        ] {
            assert!(masker.mask(printed).contains(MASK), "{printed} -> {}", masker.mask(printed));
            assert!(!masker.mask(printed).contains("w0rd") && !masker.mask(printed).contains("rd%26"), "{}", masker.mask(printed));
        }
        assert_eq!(masker.mask("ok %40 plain"), "ok %40 plain");
    }

    #[test]
    fn base64_is_masked_in_every_alignment() {
        let secret = "correct horse battery staple";
        let masker = Masker::new([secret]);
        for prefix in ["", "a", "ab", "abc"] {
            for suffix in ["", "z", "zz"] {
                let encoded = encode(format!("{prefix}{secret}{suffix}").as_bytes());
                let output = masker.mask(&encoded);
                assert!(output.contains(MASK), "{prefix:?}+{suffix:?}: {encoded} → {output}");
                assert!(output.len() < encoded.len() / 2 + 8, "most of the encoding is masked: {output}");
            }
        }
        // `user:password` for HTTP basic authentication.
        let basic = Masker::new(["hunter2hunter2"]);
        assert!(basic.mask(&encode(b"x-access-token:hunter2hunter2")).contains(MASK));
    }

    #[test]
    fn values_are_added_as_the_task_runs() {
        let masker = Masker::new([]);
        assert_eq!(masker.mask("p4ssw0rd"), "p4ssw0rd");
        assert!(masker.add("p4ssw0rd"));
        assert!(masker.add("  "), "blank values are ignored, not refused");
        assert_eq!(masker.mask("p4ssw0rd and p4ssw0rd"), "*** and ***");
        assert_eq!(masker.mask("   "), "   ");
    }
}
