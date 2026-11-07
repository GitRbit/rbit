//! GitHub's filter-pattern dialect for `branches`, `tags` and `paths` filters, ported from
//! act's `pkg/workflowpattern` (MIT, see `THIRD-PARTY.md`) so that rbit, Forgejo and act
//! agree on which pushes start a workflow.
//!
//! A pattern compiles to an anchored regular expression:
//!
//! * `*` matches within one path segment (`[^/]*`), `**` across segments (`.*`), and `**/`
//!   any number of leading directories, including none (`(.+/)?`);
//! * `+` and `?` repeat the previous character (at the start of a pattern they are literal);
//! * `[...]` is a character class of `a-z`, `A-Z`, `0-9` and ranges between them;
//! * `\` escapes the next character, and a leading `!` negates the pattern.
//!
//! Lists are evaluated as on GitHub, which is *not* glob-set semantics: for an inclusion
//! list (`branches`, `tags`, `paths`) the **last** matching pattern decides, so
//! `["*.md", "!README.md"]` excludes `README.md`; for an ignore list (`branches-ignore`,
//! `paths-ignore`) the **first** pattern that decides wins. See [`Patterns::skip`] and
//! [`Patterns::filter`].

use std::fmt;

use regex_automata::meta::Regex;

/// Upper bound for one compiled pattern's automaton. Workflow files are already capped in
/// size; this keeps a hostile pattern from costing more than a fraction of a megabyte.
const MAX_REGEX_BYTES: usize = 256 * 1024;

/// Why a pattern was rejected: every problem with its byte position, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    pub pattern: String,
    pub problems: Vec<(usize, &'static str)>,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid pattern '{}': ", self.pattern)?;
        for (index, (position, problem)) in self.problems.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "position {position}: {problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PatternError {}

/// One compiled filter pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// The pattern without its leading `!`.
    source: String,
    negative: bool,
    regex: Regex,
}

impl Pattern {
    pub fn compile(raw: &str) -> Result<Self, PatternError> {
        let (negative, source) = match raw.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };
        let expression = to_regex(source)?;
        let regex = Regex::builder()
            .configure(Regex::config().nfa_size_limit(Some(MAX_REGEX_BYTES)))
            .build(&expression)
            .map_err(|_| PatternError { pattern: source.to_owned(), problems: vec![(0, "the pattern is not a valid expression")] })?;
        Ok(Self { source: source.to_owned(), negative, regex })
    }

    /// The pattern as written, without a leading `!`.
    pub fn as_str(&self) -> &str {
        &self.source
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// Whether the whole of `input` matches the pattern (ignoring its negation).
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }
}

/// Translates a pattern into an anchored regular expression; act's `PatternToRegex`.
fn to_regex(pattern: &str) -> Result<String, PatternError> {
    let bytes = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len() * 2 + 2);
    let mut problems = Vec::new();
    let mut pos = 0;
    // What the expression ends with, so that a repetition of a repetition can be refused as
    // Go's RE2 refuses it (`*+`, `++`, `*??`); a `?` after a plain repetition makes it lazy.
    let mut last = Repetition::None;
    out.push('^');
    while pos < bytes.len() {
        let mut repetition = Repetition::None;
        match bytes[pos] {
            b'*' => {
                if bytes.get(pos + 1) == Some(&b'*') {
                    if bytes.get(pos + 2) == Some(&b'/') {
                        out.push_str("(.+/)?");
                        pos += 3;
                    } else {
                        out.push_str(".*");
                        pos += 2;
                    }
                } else {
                    out.push_str("[^/]*");
                    pos += 1;
                }
                repetition = Repetition::Greedy;
            }
            b'+' | b'?' => {
                if pos > 0 {
                    repetition = match (bytes[pos], last) {
                        (_, Repetition::None) => Repetition::Greedy,
                        (b'?', Repetition::Greedy) => Repetition::Lazy,
                        _ => {
                            problems.push((pos, "invalid nested repetition operator"));
                            Repetition::None
                        }
                    };
                    out.push(char::from(bytes[pos]));
                } else {
                    push_literal(&mut out, pattern, pos);
                }
                pos += 1;
            }
            b'[' => {
                out.push('[');
                pos += 1;
                if bytes.get(pos) == Some(&b']') {
                    problems.push((pos, "unexpected empty brackets '[]'"));
                    pos += 1;
                    continue;
                }
                let start = pos;
                while pos < bytes.len() && bytes[pos] != b']' {
                    if bytes[pos] == b'-' {
                        if pos <= start || pos + 1 >= bytes.len() {
                            problems.push((pos, "invalid range"));
                            pos += 1;
                            continue;
                        }
                        let (from, to) = (bytes[pos - 1], bytes[pos + 1]);
                        let range = |a: u8, b: u8| (a..=b).contains(&from) && (a..=b).contains(&to) && from <= to;
                        if !range(b'A', b'z') && !range(b'0', b'9') {
                            problems.push((pos, "ranges can only include a-z, A-Z, A-z, and 0-9"));
                            pos += 1;
                            continue;
                        }
                        out.push('-');
                        out.push(char::from(to));
                        pos += 2;
                    } else {
                        if !(b'A'..=b'z').contains(&bytes[pos]) && !bytes[pos].is_ascii_digit() {
                            problems.push((pos, "ranges can only include a-z, A-Z and 0-9"));
                            pos += 1;
                            continue;
                        }
                        push_literal(&mut out, pattern, pos);
                        pos += 1;
                    }
                }
                if pos >= bytes.len() || bytes[pos] != b']' {
                    problems.push((pos, "missing closing bracket ']' after '['"));
                }
                out.push(']');
                pos += 1;
            }
            b'\\' => {
                if pos + 1 >= bytes.len() {
                    problems.push((pos, "missing symbol after \\"));
                    pos += 1;
                    continue;
                }
                pos += push_literal(&mut out, pattern, pos + 1) + 1;
            }
            _ => pos += push_literal(&mut out, pattern, pos),
        }
        last = repetition;
    }
    if !problems.is_empty() {
        return Err(PatternError { pattern: pattern.to_owned(), problems });
    }
    out.push('$');
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repetition {
    None,
    Greedy,
    Lazy,
}

/// Appends the character starting at byte `pos` of `pattern`, escaped; returns its length.
/// act walks bytes, which for UTF-8 input is the same as walking characters here: every
/// special character is ASCII, and a multi-byte character is always copied whole.
fn push_literal(out: &mut String, pattern: &str, pos: usize) -> usize {
    let Some(c) = pattern.get(pos..).and_then(|rest| rest.chars().next()) else {
        // Not a character boundary: only reachable inside a class, where the byte was
        // already reported as invalid. Nothing is appended.
        return 1;
    };
    if regex_syntax_meta(c) {
        out.push('\\');
    }
    out.push(c);
    c.len_utf8()
}

/// The characters Go's `regexp.QuoteMeta` escapes.
fn regex_syntax_meta(c: char) -> bool {
    matches!(c, '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$')
}

/// A compiled filter list, in the order it was written.
#[derive(Debug, Clone, Default)]
pub struct Patterns(Vec<Pattern>);

impl Patterns {
    pub fn compile<I, S>(patterns: I) -> Result<Self, PatternError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        patterns.into_iter().map(|pattern| Pattern::compile(pattern.as_ref())).collect::<Result<_, _>>().map(Self)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pattern> {
        self.0.iter()
    }

    /// For an inclusion list (`branches`, `tags`, `paths`): `true` when the workflow is
    /// skipped because **no** input is included. An input is included when the last
    /// pattern matching it is positive. An empty list skips nothing. act's `Skip`.
    pub fn skip<I, S>(&self, inputs: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if self.0.is_empty() {
            return false;
        }
        for input in inputs {
            let input = input.as_ref();
            let included = self.0.iter().rev().find(|pattern| pattern.is_match(input)).is_some_and(|pattern| !pattern.negative);
            if included {
                return false;
            }
        }
        true
    }

    /// For an ignore list (`branches-ignore`, `tags-ignore`, `paths-ignore`): `true` when
    /// the workflow is skipped because **every** input is ignored. An input is ignored when
    /// a positive pattern matches it or a negative pattern does not, whichever comes first.
    /// An empty list skips nothing. act's `Filter`.
    pub fn filter<I, S>(&self, inputs: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if self.0.is_empty() {
            return false;
        }
        inputs.into_iter().all(|input| {
            let input = input.as_ref();
            self.0.iter().any(|pattern| pattern.is_match(input) != pattern.negative)
        })
    }
}

#[cfg(test)]
mod tests;
