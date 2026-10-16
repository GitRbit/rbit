//! The `linguist-*` subset of a root `.gitattributes` file.
//!
//! Supported attributes (each may be set, unset with `-`/`!`, or given `=true`/`=false`):
//! `linguist-vendored`, `linguist-generated`, `linguist-documentation`,
//! `linguist-detectable`, and `linguist-language=Name`.
//!
//! Patterns follow Git's rules for the common cases: a pattern without a slash matches the
//! file name at any depth; a pattern containing a slash is anchored at the repository root;
//! `*` and `?` stay within one path segment and `**` spans any number of segments. As a
//! convenience (Git itself does not do this), a pattern ending in `/` matches everything
//! below that directory. Later lines override earlier ones, attribute by attribute.

/// The overrides that apply to one path. `None` means "not mentioned": defaults apply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinguistOverrides {
    pub vendored: Option<bool>,
    pub generated: Option<bool>,
    pub documentation: Option<bool>,
    pub detectable: Option<bool>,
    pub language: Option<String>,
}

impl LinguistOverrides {
    fn apply(&mut self, setting: &Setting) {
        match setting {
            Setting::Vendored(value) => self.vendored = Some(*value),
            Setting::Generated(value) => self.generated = Some(*value),
            Setting::Documentation(value) => self.documentation = Some(*value),
            Setting::Detectable(value) => self.detectable = Some(*value),
            Setting::Language(name) => self.language = Some(name.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Setting {
    Vendored(bool),
    Generated(bool),
    Documentation(bool),
    Detectable(bool),
    Language(String),
}

#[derive(Debug, Clone)]
struct Rule {
    pattern: Pattern,
    settings: Vec<Setting>,
}

/// Parsed `linguist-*` rules, in file order.
#[derive(Debug, Clone, Default)]
pub struct LinguistAttributes {
    rules: Vec<Rule>,
}

impl LinguistAttributes {
    /// Parses `.gitattributes` text. Lines without any `linguist-*` attribute are ignored.
    pub fn parse(text: &str) -> Self {
        let rules = text
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let mut words = line.split_whitespace();
                let pattern = Pattern::parse(words.next()?)?;
                let settings: Vec<Setting> = words.filter_map(parse_setting).collect();
                (!settings.is_empty()).then_some(Rule { pattern, settings })
            })
            .collect();
        Self { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The combined overrides for `path` (slash-separated, relative to the root).
    pub fn for_path(&self, path: &str) -> LinguistOverrides {
        let mut overrides = LinguistOverrides::default();
        for rule in self.rules.iter().filter(|rule| rule.pattern.matches(path)) {
            for setting in &rule.settings {
                overrides.apply(setting);
            }
        }
        overrides
    }
}

/// Reads one `attr`, `-attr`, `!attr` or `attr=value` word.
fn parse_setting(word: &str) -> Option<Setting> {
    let (name, value) = if let Some(name) = word.strip_prefix(['-', '!']) {
        (name, Some(false))
    } else if let Some((name, value)) = word.split_once('=') {
        if name == "linguist-language" {
            return (!value.is_empty()).then(|| Setting::Language(value.replace('_', " ")));
        }
        (name, parse_bool(value))
    } else {
        (word, Some(true))
    };
    let value = value?;
    match name {
        "linguist-vendored" => Some(Setting::Vendored(value)),
        "linguist-generated" => Some(Setting::Generated(value)),
        "linguist-documentation" => Some(Setting::Documentation(value)),
        "linguist-detectable" => Some(Setting::Detectable(value)),
        _ => None,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// A compiled `.gitattributes` path pattern.
#[derive(Debug, Clone)]
struct Pattern {
    segments: Vec<String>,
    /// Contains a slash: matched against the whole path rather than the file name.
    anchored: bool,
    /// Ends in `/`: matches everything below the directory.
    directory: bool,
}

impl Pattern {
    fn parse(raw: &str) -> Option<Self> {
        let directory = raw.ends_with('/');
        let trimmed = raw.trim_end_matches('/');
        let anchored = trimmed.contains('/');
        let trimmed = trimmed.trim_start_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        Some(Self { segments: trimmed.split('/').map(str::to_owned).collect(), anchored, directory })
    }

    fn matches(&self, path: &str) -> bool {
        let path: Vec<&str> = path.split('/').collect();
        if self.directory {
            // Any proper ancestor directory of the file may match.
            return (1..path.len()).any(|end| self.matches_segments(&path[..end]));
        }
        self.matches_segments(&path)
    }

    fn matches_segments(&self, path: &[&str]) -> bool {
        if self.anchored {
            match_segments(&self.segments, path)
        } else {
            // A slash-less pattern has exactly one segment and applies to the last name.
            path.last().is_some_and(|name| match_segment(&self.segments[0], name))
        }
    }
}

/// Matches pattern segments against path segments, `**` spanning zero or more segments.
fn match_segments(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => (0..=path.len()).any(|skip| match_segments(rest, &path[skip..])),
        Some((first, rest)) => {
            path.split_first().is_some_and(|(name, remaining)| match_segment(first, name) && match_segments(rest, remaining))
        }
    }
}

/// Matches one segment with `*` (any run of characters) and `?` (one character).
fn match_segment(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    // Position of the last `*` and the name index it is currently absorbing up to.
    let mut backtrack: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                backtrack = Some((p, n));
                p += 1;
            }
            Some(&c) if c == '?' || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match backtrack {
                Some((star, absorbed)) => {
                    p = star + 1;
                    n = absorbed + 1;
                    backtrack = Some((star, absorbed + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        Pattern::parse(pattern).unwrap().matches(path)
    }

    #[test]
    fn segment_wildcards() {
        assert!(match_segment("*.js", "app.min.js"));
        assert!(match_segment("a?c", "abc"));
        assert!(!match_segment("a?c", "ac"));
        assert!(match_segment("*", ""));
        assert!(match_segment("*min*", "app.min.js"));
        assert!(!match_segment("*.ts", "app.tsx"));
    }

    #[test]
    fn slashless_patterns_match_names_at_any_depth() {
        assert!(matches("*.sql", "db/schema.sql"));
        assert!(matches("schema.sql", "schema.sql"));
        assert!(!matches("*.sql", "db/schema.sqlx"));
    }

    #[test]
    fn slashed_patterns_are_anchored() {
        assert!(matches("src/*.rs", "src/main.rs"));
        assert!(!matches("src/*.rs", "src/bin/main.rs"));
        assert!(!matches("src/*.rs", "lib/src/main.rs"));
        assert!(matches("/build.rs", "build.rs"));
        assert!(!matches("/build.rs", "sub/build.rs"));
    }

    #[test]
    fn double_star_spans_directories() {
        assert!(matches("vendor/**", "vendor/a/b/c.go"));
        assert!(matches("**/generated/*.rs", "generated/x.rs"));
        assert!(matches("**/generated/*.rs", "a/b/generated/x.rs"));
        assert!(matches("docs/**/*.html", "docs/api/v1/index.html"));
        assert!(!matches("docs/**/*.html", "site/docs/index.html"));
    }

    #[test]
    fn directory_patterns_cover_their_contents() {
        assert!(matches("third/", "third/lib/x.c"));
        assert!(matches("assets/js/", "assets/js/app.js"));
        assert!(!matches("third/", "third"));
        assert!(!matches("assets/js/", "other/assets/js/app.js"));
    }

    #[test]
    fn later_lines_override_earlier_ones() {
        let attributes = LinguistAttributes::parse(
            "# comment\n\
             *.js linguist-vendored\n\
             src/*.js -linguist-vendored\n\
             *.sql linguist-detectable=true text\n\
             *.inc linguist-language=PHP\n\
             gen/** linguist-generated !linguist-documentation\n\
             *.txt eol=lf\n",
        );
        assert!(!attributes.is_empty());
        assert_eq!(attributes.for_path("lib/a.js").vendored, Some(true));
        assert_eq!(attributes.for_path("src/a.js").vendored, Some(false));
        assert_eq!(attributes.for_path("db/a.sql").detectable, Some(true));
        assert_eq!(attributes.for_path("x/y.inc").language.as_deref(), Some("PHP"));
        let generated = attributes.for_path("gen/out.rs");
        assert_eq!((generated.generated, generated.documentation), (Some(true), Some(false)));
        assert_eq!(attributes.for_path("notes.txt"), LinguistOverrides::default());
    }

    #[test]
    fn language_names_may_use_underscores_for_spaces() {
        let attributes = LinguistAttributes::parse("*.vb linguist-language=Visual_Basic_.NET\n");
        assert_eq!(attributes.for_path("a.vb").language.as_deref(), Some("Visual Basic .NET"));
    }
}
