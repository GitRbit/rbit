//! Problem matchers: `::add-matcher::<file>` (and the legacy `##[add-matcher]<file>`) turns
//! lines a step prints into annotations, as on GitHub, until `::remove-matcher owner=…::`.
//!
//! A matcher file holds `{"problemMatcher": [{"owner", "severity"?, "pattern": [...]}]}`.
//! Each pattern is a `regexp` and the group numbers of `file`, `line`, `column`,
//! `severity`, `message` and `code`. A matcher of several patterns matches that many
//! consecutive lines; its last pattern may `loop`, giving one problem per line it matches
//! after the others. Matchers stay for the rest of the job, in every later step; a matcher
//! with the owner of one already added replaces it. The first matcher that matches a line
//! wins. A problem becomes a `##[error]`, `##[warning]` or `##[notice]` line after the
//! printed one, as an `::error file=…::` command does.

use regex_automata::meta::Regex;
use serde::Deserialize;

/// Most matchers a job may hold, and most patterns a matcher may have.
const MAX_MATCHERS: usize = 64;
const MAX_PATTERNS: usize = 16;
/// Longest line a matcher is run on: what is longer is printed, never matched.
const MAX_MATCHED_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
struct File {
    #[serde(rename = "problemMatcher")]
    matchers: Vec<Definition>,
}

#[derive(Debug, Deserialize)]
struct Definition {
    owner: String,
    #[serde(default)]
    severity: Option<String>,
    pattern: Vec<PatternDefinition>,
}

#[derive(Debug, Deserialize)]
struct PatternDefinition {
    regexp: String,
    file: Option<usize>,
    line: Option<usize>,
    column: Option<usize>,
    severity: Option<usize>,
    message: Option<usize>,
    code: Option<usize>,
    #[serde(default, rename = "loop")]
    repeat: bool,
}

#[derive(Debug)]
struct Pattern {
    regex: Regex,
    groups: PatternDefinition,
}

#[derive(Debug)]
struct Matcher {
    owner: String,
    severity: Option<String>,
    patterns: Vec<Pattern>,
    /// The next pattern a line must match, and what earlier ones captured.
    next: usize,
    captured: Captured,
}

/// The fields of a problem, filled pattern by pattern.
#[derive(Debug, Clone, Default)]
struct Captured {
    file: Option<String>,
    line: Option<String>,
    column: Option<String>,
    severity: Option<String>,
    message: Option<String>,
    code: Option<String>,
}

/// The matchers of one job.
#[derive(Debug, Default)]
pub struct Matchers {
    list: Vec<Matcher>,
}

impl Matchers {
    /// Adds the matchers of a matcher file's `text`; the owners added.
    pub fn add(&mut self, text: &str) -> Result<Vec<String>, String> {
        let file: File = serde_json::from_str(text).map_err(|error| format!("not a problem matcher file: {error}"))?;
        let mut added = Vec::new();
        for definition in file.matchers {
            if definition.pattern.is_empty() || definition.pattern.len() > MAX_PATTERNS {
                return Err(format!("matcher `{}` needs 1 to {MAX_PATTERNS} patterns", definition.owner));
            }
            let mut patterns = Vec::new();
            for (index, groups) in definition.pattern.into_iter().enumerate() {
                if groups.repeat && index == 0 {
                    return Err(format!("matcher `{}`: only a later pattern can loop", definition.owner));
                }
                let regex = Regex::new(&groups.regexp).map_err(|error| format!("matcher `{}`: {error}", definition.owner))?;
                patterns.push(Pattern { regex, groups });
            }
            if !patterns.iter().any(|pattern| pattern.groups.message.is_some()) {
                return Err(format!("matcher `{}` has no `message`", definition.owner));
            }
            self.remove(&definition.owner);
            if self.list.len() >= MAX_MATCHERS {
                return Err(format!("a job may have at most {MAX_MATCHERS} problem matchers"));
            }
            added.push(definition.owner.clone());
            self.list.push(Matcher {
                owner: definition.owner,
                severity: definition.severity,
                patterns,
                next: 0,
                captured: Captured::default(),
            });
        }
        Ok(added)
    }

    /// Removes the matcher of `owner`.
    pub fn remove(&mut self, owner: &str) {
        self.list.retain(|matcher| matcher.owner != owner);
    }

    /// The annotation lines `line` gives, if a matcher matches it.
    pub fn annotate(&mut self, line: &str) -> Option<String> {
        if self.list.is_empty() || line.len() > MAX_MATCHED_BYTES {
            return None;
        }
        // Every matcher sees every line, so none loses its place in a multi-line problem;
        // the first one's problem is shown.
        let mut found = None;
        for matcher in &mut self.list {
            let problem = matcher.feed(line);
            if found.is_none() {
                found = problem;
            }
        }
        found
    }
}

impl Matcher {
    /// Moves the matcher along `line`; a finished problem's annotation.
    fn feed(&mut self, line: &str) -> Option<String> {
        let looping = self.next > 0 && self.next == self.patterns.len() - 1 && self.patterns[self.next].groups.repeat;
        if let Some(fields) = self.patterns[self.next].matches(line) {
            return self.advance(&fields);
        }
        // A line that breaks the sequence (or ends a loop) may start a new one.
        let restarted = self.next != 0 || looping;
        self.next = 0;
        self.captured = Captured::default();
        if restarted && let Some(fields) = self.patterns[0].matches(line) {
            return self.advance(&fields);
        }
        None
    }

    fn advance(&mut self, fields: &Captured) -> Option<String> {
        let current = self.next;
        let mut problem = self.captured.clone();
        problem.merge(fields);
        if current + 1 < self.patterns.len() {
            self.captured = problem;
            self.next = current + 1;
            return None;
        }
        if self.patterns[current].groups.repeat {
            // The next line may match the last pattern again, with the earlier lines' fields.
            self.next = current;
        } else {
            self.next = 0;
            self.captured = Captured::default();
        }
        annotation(&problem, self.severity.as_deref())
    }
}

impl Pattern {
    fn matches(&self, line: &str) -> Option<Captured> {
        let mut captures = self.regex.create_captures();
        self.regex.captures(line, &mut captures);
        if !captures.is_match() {
            return None;
        }
        let group = |index: Option<usize>| {
            index
                .and_then(|index| captures.get_group(index))
                .map(|span| line[span.start..span.end].to_owned())
                .filter(|text| !text.is_empty())
        };
        Some(Captured {
            file: group(self.groups.file),
            line: group(self.groups.line),
            column: group(self.groups.column),
            severity: group(self.groups.severity),
            message: group(self.groups.message),
            code: group(self.groups.code),
        })
    }
}

impl Captured {
    fn merge(&mut self, other: &Self) {
        for (mine, theirs) in [
            (&mut self.file, &other.file),
            (&mut self.line, &other.line),
            (&mut self.column, &other.column),
            (&mut self.severity, &other.severity),
            (&mut self.message, &other.message),
            (&mut self.code, &other.code),
        ] {
            if theirs.is_some() {
                mine.clone_from(theirs);
            }
        }
    }
}

/// The `##[kind]message (params)` line of a problem: its severity, else the matcher's,
/// else an error.
fn annotation(problem: &Captured, default_severity: Option<&str>) -> Option<String> {
    let message = problem.message.as_deref()?;
    let severity = problem.severity.as_deref().or(default_severity).unwrap_or("error").to_ascii_lowercase();
    let kind = if severity.starts_with("warn") {
        "warning"
    } else if severity.starts_with("notice") || severity.starts_with("info") {
        "notice"
    } else {
        "error"
    };
    let params: Vec<String> = [("file", &problem.file), ("line", &problem.line), ("col", &problem.column), ("code", &problem.code)]
        .into_iter()
        .filter_map(|(name, value)| value.as_ref().map(|value| format!("{name}={value}")))
        .collect();
    Some(if params.is_empty() { format!("##[{kind}]{message}") } else { format!("##[{kind}]{message} ({})", params.join(",")) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GCC: &str = r#"{"problemMatcher":[{"owner":"gcc","pattern":[{"regexp":"^(.+):(\\d+):(\\d+): (warning|error): (.+)$","file":1,"line":2,"column":3,"severity":4,"message":5}]}]}"#;
    const ESLINT: &str = r#"{"problemMatcher":[{"owner":"eslint-stylish","pattern":[{"regexp":"^([^\\s].*)$","file":1},{"regexp":"^\\s+(\\d+):(\\d+)\\s+(error|warning|info)\\s+(.*)\\s\\s+(.*)$","line":1,"column":2,"severity":3,"message":4,"code":5,"loop":true}]}]}"#;

    #[test]
    fn one_line_problems_become_annotations_until_removed() {
        let mut matchers = Matchers::default();
        assert_eq!(matchers.add(GCC).unwrap(), ["gcc"]);
        assert_eq!(
            matchers.annotate("src/main.c:12:5: error: expected ';'").as_deref(),
            Some("##[error]expected ';' (file=src/main.c,line=12,col=5)")
        );
        assert_eq!(matchers.annotate("lib.c:1:2: warning: unused").as_deref(), Some("##[warning]unused (file=lib.c,line=1,col=2)"));
        assert_eq!(matchers.annotate("compiling…"), None);
        matchers.remove("gcc");
        assert_eq!(matchers.annotate("src/main.c:12:5: error: expected ';'"), None);
    }

    #[test]
    fn a_looping_pattern_gives_one_problem_per_line() {
        let mut matchers = Matchers::default();
        matchers.add(ESLINT).unwrap();
        assert_eq!(matchers.annotate("src/app.js"), None);
        assert_eq!(
            matchers.annotate("  3:7  error  'x' is never used  no-unused-vars").as_deref(),
            Some("##[error]'x' is never used (file=src/app.js,line=3,col=7,code=no-unused-vars)")
        );
        assert_eq!(
            matchers.annotate("  9:1  warning  Unexpected console  no-console").as_deref(),
            Some("##[warning]Unexpected console (file=src/app.js,line=9,col=1,code=no-console)")
        );
        // A new file name ends the loop and starts the next sequence.
        assert_eq!(matchers.annotate("src/other.js"), None);
        assert_eq!(
            matchers.annotate("  1:1  error  Missing semicolon  semi").as_deref(),
            Some("##[error]Missing semicolon (file=src/other.js,line=1,col=1,code=semi)")
        );
    }

    #[test]
    fn bad_matcher_files_are_refused_and_owners_replace() {
        let mut matchers = Matchers::default();
        assert!(matchers.add("{}").is_err());
        assert!(matchers.add(r#"{"problemMatcher":[{"owner":"x","pattern":[{"regexp":"(","message":1}]}]}"#).is_err());
        assert!(matchers.add(r#"{"problemMatcher":[{"owner":"x","pattern":[{"regexp":"a"}]}]}"#).is_err(), "no message");
        matchers.add(GCC).unwrap();
        matchers.add(r#"{"problemMatcher":[{"owner":"gcc","severity":"warning","pattern":[{"regexp":"^W (.+)$","message":1}]}]}"#).unwrap();
        assert_eq!(matchers.list.len(), 1, "the same owner replaces");
        assert_eq!(matchers.annotate("W careful").as_deref(), Some("##[warning]careful"));
    }
}
