//! Path globs as `actions/upload-artifact` and `actions/cache` read their `path:` input,
//! which is GitHub's `@actions/glob`: one pattern per line, the last matching pattern
//! deciding, `!` lines excluding.
//!
//! * Segments are separated by `/` (or `\`). `*` matches any run of characters within a
//!   segment, `?` one character, `[abc]`, `[a-z]` and `[!a]` one of a set; `**` alone in a
//!   segment matches any number of segments, none included. Like `@actions/glob` (minimatch
//!   with `dot: true`), `*` also matches names that start with a dot.
//! * A pattern matches what lies below a directory it matches too (implicit descendants):
//!   `dist` is `dist` and everything in it, and `!dist/tmp` leaves out all of `dist/tmp`.
//! * Patterns are relative to the root they are matched below; `..` is refused, and a
//!   pattern never matches outside its root.
//! * [`GlobSet::search_paths`] are the directories (or files) worth walking: each positive
//!   pattern's segments up to its first wildcard, without those inside another one. They
//!   also decide an artifact's root (the least common ancestor), as on GitHub.
//!
//! Pure: matching takes `/`-separated relative paths, and the runner walks the file system.

use std::fmt;

/// The most patterns a `path:` input may hold.
pub const MAX_PATTERNS: usize = 256;
/// The longest pattern.
pub const MAX_PATTERN_BYTES: usize = 4096;

/// Why a pattern was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobError(pub String);

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GlobError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Char(char),
    /// `*`
    Any,
    /// `?`
    One,
    /// `[...]`: ranges, and whether it is negated (`[!...]` or `[^...]`).
    Class(Vec<(char, char)>, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `**`
    Globstar,
    Tokens(Vec<Token>),
}

impl Segment {
    fn is_literal(&self) -> bool {
        matches!(self, Self::Tokens(tokens) if tokens.iter().all(|token| matches!(token, Token::Char(_))))
    }

    fn literal(&self) -> Option<String> {
        match self {
            Self::Tokens(tokens) => tokens
                .iter()
                .map(|token| match token {
                    Token::Char(c) => Some(*c),
                    _ => None,
                })
                .collect(),
            Self::Globstar => None,
        }
    }
}

/// One line of a `path:` input.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glob {
    negate: bool,
    segments: Vec<Segment>,
}

/// The patterns of a `path:` input, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobSet {
    globs: Vec<Glob>,
    case_insensitive: bool,
}

impl GlobSet {
    /// Compiles `lines` (blank lines skipped). `case_insensitive` for Windows file systems.
    ///
    /// # Errors
    ///
    /// A line that is empty after `!`, too long, leaves its root (`..`), or has an unclosed
    /// `[`, and more than [`MAX_PATTERNS`] lines.
    pub fn compile<'a>(lines: impl IntoIterator<Item = &'a str>, case_insensitive: bool) -> Result<Self, GlobError> {
        let mut globs = Vec::new();
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if globs.len() >= MAX_PATTERNS {
                return Err(GlobError(format!("at most {MAX_PATTERNS} patterns are allowed")));
            }
            if line.len() > MAX_PATTERN_BYTES {
                return Err(GlobError(format!("a pattern is longer than {MAX_PATTERN_BYTES} bytes")));
            }
            let (negate, body) = match line.strip_prefix('!') {
                Some(rest) => (true, rest.trim_start()),
                None => (false, line),
            };
            let segments = parse(body).map_err(|why| GlobError(format!("`{line}`: {why}")))?;
            if segments.is_empty() {
                return Err(GlobError(format!("`{line}` names nothing")));
            }
            globs.push(Glob { negate, segments });
        }
        Ok(Self { globs, case_insensitive })
    }

    pub fn is_empty(&self) -> bool {
        !self.globs.iter().any(|glob| !glob.negate)
    }

    /// Whether `path` (relative, `/`-separated) is selected: the last pattern matching it
    /// or one of its ancestors decides.
    pub fn matches(&self, path: &str) -> bool {
        let parts = split(path);
        let mut selected = false;
        for glob in &self.globs {
            if (0..=parts.len()).any(|end| end > 0 && self.match_all(&glob.segments, &parts[..end])) {
                selected = !glob.negate;
            }
        }
        selected
    }

    /// Whether something below directory `dir` could be selected, so walking it is worth it.
    pub fn may_contain(&self, dir: &str) -> bool {
        let parts = split(dir);
        self.globs.iter().filter(|glob| !glob.negate).any(|glob| {
            (1..=parts.len()).any(|end| self.match_all(&glob.segments, &parts[..end])) || self.match_prefix(&glob.segments, &parts)
        })
    }

    /// The literal start of every positive pattern (up to its first wildcard segment),
    /// `/`-separated, without one that lies inside another; `""` is the root.
    pub fn search_paths(&self) -> Vec<String> {
        let mut paths: Vec<Vec<String>> = Vec::new();
        for glob in self.globs.iter().filter(|glob| !glob.negate) {
            let literal: Vec<String> = glob.segments.iter().map_while(Segment::literal).collect();
            if !paths.contains(&literal) {
                paths.push(literal);
            }
        }
        let inside = |path: &Vec<String>, other: &Vec<String>| other.len() < path.len() && path.starts_with(other);
        let kept: Vec<String> =
            paths.iter().filter(|path| !paths.iter().any(|other| inside(path, other))).map(|path| path.join("/")).collect();
        kept
    }

    /// Whether every segment of the patterns is literal: no `*`, `?`, `[` or `**`.
    pub fn is_literal(&self) -> bool {
        self.globs.iter().all(|glob| glob.segments.iter().all(Segment::is_literal))
    }

    fn match_all(&self, segments: &[Segment], parts: &[&str]) -> bool {
        match segments.split_first() {
            None => parts.is_empty(),
            Some((Segment::Globstar, rest)) => (0..=parts.len()).any(|skip| self.match_all(rest, &parts[skip..])),
            Some((Segment::Tokens(tokens), rest)) => {
                parts.split_first().is_some_and(|(part, others)| self.match_segment(tokens, part) && self.match_all(rest, others))
            }
        }
    }

    /// Whether `parts` (a directory) could lead to a match of `segments`: they match a
    /// prefix of the pattern.
    fn match_prefix(&self, segments: &[Segment], parts: &[&str]) -> bool {
        match (segments.split_first(), parts.split_first()) {
            (_, None) | (Some((Segment::Globstar, _)), Some(_)) => true,
            (None, Some(_)) => false,
            (Some((Segment::Tokens(tokens), rest)), Some((part, others))) => {
                self.match_segment(tokens, part) && self.match_prefix(rest, others)
            }
        }
    }

    fn match_segment(&self, tokens: &[Token], name: &str) -> bool {
        let name: Vec<char> = name.chars().collect();
        self.match_tokens(tokens, &name)
    }

    fn match_tokens(&self, tokens: &[Token], name: &[char]) -> bool {
        let same = |a: char, b: char| if self.case_insensitive { a.to_lowercase().eq(b.to_lowercase()) } else { a == b };
        match tokens.split_first() {
            None => name.is_empty(),
            Some((Token::Any, rest)) => (0..=name.len()).any(|skip| self.match_tokens(rest, &name[skip..])),
            Some((token, rest)) => name.split_first().is_some_and(|(&c, others)| {
                let hit = match token {
                    Token::Char(expected) => same(*expected, c),
                    Token::One => true,
                    Token::Class(ranges, negated) => {
                        let lower = c.to_lowercase().next().unwrap_or(c);
                        let upper = c.to_uppercase().next().unwrap_or(c);
                        let inside = |c: char| ranges.iter().any(|&(low, high)| low <= c && c <= high);
                        let found = inside(c) || (self.case_insensitive && (inside(lower) || inside(upper)));
                        found != *negated
                    }
                    Token::Any => unreachable!(),
                };
                hit && self.match_tokens(rest, others)
            }),
        }
    }
}

fn split(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|part| !part.is_empty() && *part != ".").collect()
}

fn parse(pattern: &str) -> Result<Vec<Segment>, &'static str> {
    let mut segments = Vec::new();
    for part in pattern.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err("patterns may not leave their directory (`..`)"),
            "**" => {
                if segments.last() != Some(&Segment::Globstar) {
                    segments.push(Segment::Globstar);
                }
            }
            part => segments.push(Segment::Tokens(tokens(part)?)),
        }
    }
    Ok(segments)
}

fn tokens(part: &str) -> Result<Vec<Token>, &'static str> {
    let chars: Vec<char> = part.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '*' => {
                if tokens.last() != Some(&Token::Any) {
                    tokens.push(Token::Any);
                }
                at += 1;
            }
            '?' => {
                tokens.push(Token::One);
                at += 1;
            }
            '[' => {
                let mut cursor = at + 1;
                let negated = matches!(chars.get(cursor), Some('!' | '^'));
                if negated {
                    cursor += 1;
                }
                let mut ranges = Vec::new();
                let mut first = true;
                loop {
                    let Some(&c) = chars.get(cursor) else { return Err("a `[` is not closed") };
                    if c == ']' && !first {
                        break;
                    }
                    first = false;
                    if chars.get(cursor + 1) == Some(&'-') && chars.get(cursor + 2).is_some_and(|&end| end != ']') {
                        ranges.push((c, chars[cursor + 2]));
                        cursor += 3;
                    } else {
                        ranges.push((c, c));
                        cursor += 1;
                    }
                }
                tokens.push(Token::Class(ranges, negated));
                at = cursor + 1;
            }
            c => {
                tokens.push(Token::Char(c));
                at += 1;
            }
        }
    }
    Ok(tokens)
}

/// The least common ancestor of `paths` (`/`-separated, relative), segment by segment.
pub fn common_ancestor<'a>(paths: impl IntoIterator<Item = &'a str>) -> String {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let parts = split(path);
        common = Some(match common {
            None => parts,
            Some(common) => common.iter().zip(&parts).take_while(|(a, b)| a == b).map(|(a, _)| *a).collect(),
        });
    }
    common.unwrap_or_default().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(lines: &[&str]) -> GlobSet {
        GlobSet::compile(lines.iter().copied(), false).unwrap()
    }

    #[test]
    fn wildcards_match_within_a_segment_and_globstars_across() {
        let star = set(&["dist/*.zip*"]);
        assert!(star.matches("dist/a.zip") && star.matches("dist/a.zip.sha256"));
        assert!(!star.matches("dist/b.txt") && !star.matches("dist/sub/c.zip"), "`*` stays within its segment");
        let deep = set(&["dist/**/*.zip"]);
        assert!(deep.matches("dist/a.zip") && deep.matches("dist/sub/c.zip") && deep.matches("dist/x/y/z.zip"));
        assert!(!deep.matches("other/a.zip"));
        let one = set(&["out/a?.log", "src/[a-c]*.rs", "src/[!x]y.rs"]);
        assert!(one.matches("out/a1.log") && !one.matches("out/b1.log") && !one.matches("out/a12.log"));
        assert!(one.matches("src/b.rs") && !one.matches("src/d.rs"));
        assert!(one.matches("src/ay.rs") && !one.matches("src/xy.rs"));
        assert!(set(&["*"]).matches(".hidden"), "dotfiles match, as with minimatch's dot option");
    }

    #[test]
    fn the_last_matching_pattern_decides_and_directories_bring_their_contents() {
        let globs = set(&["dist/**/*.zip", "!dist/sub/skip.zip", "docs/readme.md", "target", "!target/tmp"]);
        assert!(globs.matches("dist/sub/c.zip") && !globs.matches("dist/sub/skip.zip"));
        assert!(globs.matches("docs/readme.md"));
        assert!(globs.matches("target/x/y.rlib"), "implicit descendants");
        assert!(!globs.matches("target/tmp/a") && !globs.matches("target/tmp"), "an excluded directory takes its contents");
        let again = set(&["a/**", "!a/b/**", "a/b/keep"]);
        assert!(again.matches("a/x") && !again.matches("a/b/y") && again.matches("a/b/keep"));
    }

    #[test]
    fn search_paths_are_the_literal_starts_without_nested_ones() {
        assert_eq!(set(&["dist/*.zip*"]).search_paths(), ["dist"]);
        assert_eq!(set(&["dist/**/*.zip", "!dist/sub/skip.zip", "docs/readme.md"]).search_paths(), ["dist", "docs/readme.md"]);
        assert_eq!(set(&["a/b", "a/b/c/*.txt", "a"]).search_paths(), ["a"]);
        assert_eq!(set(&["**/*.rlib"]).search_paths(), [""]);
        assert_eq!(common_ancestor(["dist", "docs/readme.md"]), "");
        assert_eq!(common_ancestor(["dist/a.txt", "dist/b.txt"]), "dist");
        assert_eq!(common_ancestor(["a/b/c", "a/b"]), "a/b");
    }

    #[test]
    fn walking_prunes_directories_that_cannot_match() {
        let globs = set(&["src/**/*.rs", "docs/*.md"]);
        assert!(globs.may_contain("src") && globs.may_contain("src/deep/er") && globs.may_contain("docs"));
        assert!(!globs.may_contain("target") && !globs.may_contain("docs/sub"));
        assert!(set(&["dist"]).may_contain("dist/any/depth"), "inside a matched directory");
    }

    #[test]
    fn bad_patterns_are_refused() {
        for bad in ["../x", "a/../b", "[abc", "!"] {
            assert!(GlobSet::compile([bad], false).is_err(), "{bad}");
        }
        assert!(GlobSet::compile(["  ", "", "x"], false).is_ok(), "blank lines are skipped");
        let many: Vec<String> = (0..=MAX_PATTERNS).map(|n| format!("f{n}")).collect();
        assert!(GlobSet::compile(many.iter().map(String::as_str), false).is_err());
    }

    #[test]
    fn windows_paths_and_case() {
        let globs = GlobSet::compile(["Dist\\*.ZIP"], true).unwrap();
        assert!(globs.matches("dist/a.zip") && globs.matches("dist\\b.Zip"));
        assert!(!set(&["Dist/*.ZIP"]).matches("dist/a.zip"), "case counts elsewhere");
    }
}
