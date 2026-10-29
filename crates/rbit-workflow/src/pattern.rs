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
mod tests {
    use super::*;

    /// act's `TestMatchPattern` table, verbatim: patterns, inputs, `Skip`, `Filter`.
    #[rustfmt::skip]
    const ACT_CASES: &[(&[&str], &[&str], bool, bool)] = &[
        (&["*"], &["path/with/slash"], true, false),
        (&["path/a", "path/b", "path/c"], &["meta", "path/b", "otherfile"], false, false),
        (&["path/a", "path/b", "path/c"], &["path/b"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b", "path/a"], false, true),
        (&["path/a", "path/b", "path/c"], &["path/c", "path/b", "path/d", "path/a"], false, false),
        (&[], &[], false, false),
        (&["\\!file"], &["!file"], false, true),
        (&["escape\\\\backslash"], &["escape\\backslash"], false, true),
        (&[".yml"], &["fyml"], true, false),
        // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-branches-and-tags
        (&["feature/*"], &["feature/my-branch"], false, true),
        (&["feature/*"], &["feature/your-branch"], false, true),
        (&["feature/**"], &["feature/beta-a/my-branch"], false, true),
        (&["feature/**"], &["feature/beta-a/my-branch"], false, true),
        (&["feature/**"], &["feature/mona/the/octocat"], false, true),
        (&["main", "releases/mona-the-octocat"], &["main"], false, true),
        (&["main", "releases/mona-the-octocat"], &["releases/mona-the-octocat"], false, true),
        (&["*"], &["main"], false, true),
        (&["*"], &["releases"], false, true),
        (&["**"], &["all/the/branches"], false, true),
        (&["**"], &["every/tag"], false, true),
        (&["*feature"], &["mona-feature"], false, true),
        (&["*feature"], &["feature"], false, true),
        (&["*feature"], &["ver-10-feature"], false, true),
        (&["v2*"], &["v2"], false, true),
        (&["v2*"], &["v2.0"], false, true),
        (&["v2*"], &["v2.9"], false, true),
        (&["v[12].[0-9]+.[0-9]+"], &["v1.10.1"], false, true),
        (&["v[12].[0-9]+.[0-9]+"], &["v2.0.0"], false, true),
        // https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#patterns-to-match-file-paths
        (&["*"], &["README.md"], false, true),
        (&["*"], &["server.rb"], false, true),
        (&["*.jsx?"], &["page.js"], false, true),
        (&["*.jsx?"], &["page.jsx"], false, true),
        (&["**"], &["all/the/files.md"], false, true),
        (&["*.js"], &["app.js"], false, true),
        (&["*.js"], &["index.js"], false, true),
        (&["**.js"], &["index.js"], false, true),
        (&["**.js"], &["js/index.js"], false, true),
        (&["**.js"], &["src/js/app.js"], false, true),
        (&["docs/*"], &["docs/README.md"], false, true),
        (&["docs/*"], &["docs/file.txt"], false, true),
        (&["docs/**"], &["docs/README.md"], false, true),
        (&["docs/**"], &["docs/mona/octocat.txt"], false, true),
        (&["docs/**/*.md"], &["docs/README.md"], false, true),
        (&["docs/**/*.md"], &["docs/mona/hello-world.md"], false, true),
        (&["docs/**/*.md"], &["docs/a/markdown/file.md"], false, true),
        (&["**/docs/**"], &["docs/hello.md"], false, true),
        (&["**/docs/**"], &["dir/docs/my-file.txt"], false, true),
        (&["**/docs/**"], &["space/docs/plan/space.doc"], false, true),
        (&["**/README.md"], &["README.md"], false, true),
        (&["**/README.md"], &["js/README.md"], false, true),
        (&["**/*src/**"], &["a/src/app.js"], false, true),
        (&["**/*src/**"], &["my-src/code/js/app.js"], false, true),
        (&["**/*-post.md"], &["my-post.md"], false, true),
        (&["**/*-post.md"], &["path/their-post.md"], false, true),
        (&["**/migrate-*.sql"], &["migrate-10909.sql"], false, true),
        (&["**/migrate-*.sql"], &["db/migrate-v1.0.sql"], false, true),
        (&["**/migrate-*.sql"], &["db/sept/migrate-v1.sql"], false, true),
        (&["*.md", "!README.md"], &["hello.md"], false, true),
        (&["*.md", "!README.md"], &["README.md"], true, true),
        (&["*.md", "!README.md"], &["docs/hello.md"], true, true),
        (&["*.md", "!README.md", "README*"], &["hello.md"], false, true),
        (&["*.md", "!README.md", "README*"], &["README.md"], false, true),
        (&["*.md", "!README.md", "README*"], &["README.doc"], false, true),
    ];

    #[test]
    fn act_match_pattern_table() {
        for (patterns, inputs, skip, filter) in ACT_CASES {
            let compiled = Patterns::compile(*patterns).unwrap();
            assert_eq!(compiled.skip(*inputs), *skip, "skip {patterns:?} {inputs:?}");
            assert_eq!(compiled.filter(*inputs), *filter, "filter {patterns:?} {inputs:?}");
        }
    }

    #[test]
    fn translation_matches_act() {
        let cases = [
            ("*", "^[^/]*$"),
            ("**", "^.*$"),
            ("**/a", "^(.+/)?a$"),
            ("a.b", "^a\\.b$"),
            ("+a?", "^\\+a?$"),
            ("v[12].[0-9]+", "^v[12]\\.[0-9]+$"),
            ("[A-z]", "^[A-z]$"),
            ("\\*", "^\\*$"),
            ("(x|y){2}", "^\\(x\\|y\\)\\{2\\}$"),
            ("é*", "^é[^/]*$"),
        ];
        for (pattern, expected) in cases {
            assert_eq!(to_regex(pattern).unwrap(), expected, "{pattern}");
        }
    }

    #[test]
    fn invalid_patterns_name_every_problem() {
        let cases: &[(&str, &[(usize, &str)])] = &[
            ("[]", &[(1, "unexpected empty brackets '[]'")]),
            ("[a", &[(2, "missing closing bracket ']' after '['")]),
            ("[-a]", &[(1, "invalid range")]),
            ("[a-]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
            ("[a-", &[(2, "invalid range"), (3, "missing closing bracket ']' after '['")]),
            ("[z-a]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
            ("[a-9]", &[(2, "ranges can only include a-z, A-Z, A-z, and 0-9")]),
            ("[.]", &[(1, "ranges can only include a-z, A-Z and 0-9")]),
            ("[é]", &[(1, "ranges can only include a-z, A-Z and 0-9"), (2, "ranges can only include a-z, A-Z and 0-9")]),
            ("a\\", &[(1, "missing symbol after \\")]),
        ];
        for (pattern, problems) in cases {
            let error = Pattern::compile(pattern).unwrap_err();
            assert_eq!(error.problems, *problems, "{pattern}");
            assert!(error.to_string().starts_with(&format!("invalid pattern '{pattern}': position ")));
        }
        // Go's RE2 refuses a repetition of a repetition; the translation does too.
        for pattern in ["a*+", "a++", "a?+", "**/+", "a*??", "a+??"] {
            assert_eq!(Pattern::compile(pattern).unwrap_err().problems.len(), 1, "{pattern}");
        }
        for pattern in ["a*?", "a+?", "a??", "**/?", "+a", "?a", "[ab]+", "a+b+"] {
            assert!(Pattern::compile(pattern).is_ok(), "{pattern}");
        }
        assert!(Pattern::compile("!").is_ok_and(|pattern| pattern.is_negative() && pattern.is_match("")));
    }

    #[test]
    fn negation_order_is_github_not_glob() {
        let list = Patterns::compile(["**", "!docs/**", "docs/keep.md"]).unwrap();
        assert!(!list.skip(["src/main.rs"]));
        assert!(list.skip(["docs/a.md"]), "excluded by the later negation");
        assert!(!list.skip(["docs/keep.md"]), "re-included by the last pattern");
        assert!(!list.skip(["docs/a.md", "src/lib.rs"]), "one included input is enough");
        let ignore = Patterns::compile(["docs/**", "!docs/keep.md"]).unwrap();
        assert!(ignore.filter(["docs/a.md"]));
        assert!(ignore.filter(["docs/keep.md"]), "the first deciding pattern wins");
        // act's (and Forgejo's) quirk, kept: a negative pattern that does not match an input
        // ignores it, so in this list every path outside `docs/keep.md` is ignored.
        assert!(ignore.filter(["docs/a.md", "src/lib.rs"]));
        let ignore = Patterns::compile(["docs/**"]).unwrap();
        assert!(!ignore.filter(["docs/a.md", "src/lib.rs"]), "one input not ignored is enough");
        assert!(!Patterns::default().skip(["x"]) && !Patterns::default().filter(["x"]));
    }
}
