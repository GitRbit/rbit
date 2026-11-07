//! The issue search language (`is:open label:bug author:alice sort:updated-desc …`),
//! parsed into a structured query. Pure.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Terms are separated by whitespace; a value may be quoted (`label:"good first issue"`).
//!
//! | Term | Meaning |
//! |---|---|
//! | `is:open`, `is:closed` | state |
//! | `is:issue`, `is:pr` | kind |
//! | `is:merged`, `is:draft`, `is:locked` | merged / draft pull requests, locked threads |
//! | `author:name`, `assignee:name` | by account (`@me`: the signed-in viewer) |
//! | `no:assignee`, `no:label`, `no:milestone` | without any |
//! | `label:name` (repeatable, all must match), `-label:name` | labels |
//! | `milestone:"title"` | milestone |
//! | `sort:created-desc` … | order (see [`IssueSort`]) |
//! | anything else | free text in the title and body |
//!
//! At most [`MAX_TERMS`] terms are read. Unknown qualifiers (`foo:bar`) are reported in
//! [`IssueQuery::ignored`] and otherwise ignored. [`IssueQuery`]'s `Display` writes the
//! canonical query back, so filter menus can change one part and link to the result.

use std::fmt;

/// Terms read from one query; the rest are ignored.
pub const MAX_TERMS: usize = 20;
/// Longest free-text word kept.
const MAX_WORD_CHARS: usize = 100;

/// What the issue list shows when the query is empty: `is:open is:issue`.
pub const DEFAULT_QUERY: &str = "is:open is:issue";

/// Open or closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
}

/// Issues or pull requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindFilter {
    Issue,
    Pull,
}

/// An account named in `author:` or `assignee:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    Name(String),
    /// `@me`: needs a signed-in viewer.
    Me,
}

impl Who {
    fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("@me") { Self::Me } else { Self::Name(value.trim_start_matches('@').to_owned()) }
    }
}

impl fmt::Display for Who {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => f.write_str(&quote(name)),
            Self::Me => f.write_str("@me"),
        }
    }
}

/// The order of results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueSort {
    #[default]
    CreatedDesc,
    CreatedAsc,
    UpdatedDesc,
    UpdatedAsc,
    CommentsDesc,
}

impl IssueSort {
    pub const ALL: [Self; 5] = [Self::CreatedDesc, Self::CreatedAsc, Self::UpdatedDesc, Self::UpdatedAsc, Self::CommentsDesc];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreatedDesc => "created-desc",
            Self::CreatedAsc => "created-asc",
            Self::UpdatedDesc => "updated-desc",
            Self::UpdatedAsc => "updated-asc",
            Self::CommentsDesc => "comments-desc",
        }
    }

    /// The menu label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::CreatedDesc => "Newest",
            Self::CreatedAsc => "Oldest",
            Self::UpdatedDesc => "Recently updated",
            Self::UpdatedAsc => "Least recently updated",
            Self::CommentsDesc => "Most commented",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sort| sort.as_str().eq_ignore_ascii_case(value))
    }
}

/// A parsed issue search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent filters")]
pub struct IssueQuery {
    pub state: Option<StateFilter>,
    pub kind: Option<KindFilter>,
    pub merged: bool,
    pub draft: bool,
    pub locked: bool,
    pub author: Option<Who>,
    pub assignee: Option<Who>,
    pub no_assignee: bool,
    /// All must be present.
    pub labels: Vec<String>,
    pub not_labels: Vec<String>,
    pub no_label: bool,
    pub milestone: Option<String>,
    pub no_milestone: bool,
    pub sort: IssueSort,
    /// Free-text words (title and body).
    pub text: Vec<String>,
    /// Terms that were not understood or not read, for a hint.
    pub ignored: Vec<String>,
}

impl IssueQuery {
    /// Parses `input`; an empty (or blank) input is [`DEFAULT_QUERY`].
    pub fn parse(input: &str) -> Self {
        let input = if input.trim().is_empty() { DEFAULT_QUERY } else { input };
        let mut query = Self::default();
        for (index, term) in split_terms(input).into_iter().enumerate() {
            if index >= MAX_TERMS {
                query.ignored.push(term);
                continue;
            }
            query.apply(term);
        }
        query
    }

    fn apply(&mut self, term: String) {
        let (negated, body) = term.strip_prefix('-').map_or((false, term.as_str()), |rest| (true, rest));
        let Some((key, value)) = body.split_once(':').filter(|(key, value)| !key.is_empty() && !value.is_empty()) else {
            let word: String = term.chars().take(MAX_WORD_CHARS).collect();
            self.text.push(word);
            return;
        };
        let key = key.to_ascii_lowercase();
        let lower = value.to_ascii_lowercase();
        match (negated, key.as_str(), lower.as_str()) {
            (false, "is", "open") => self.state = Some(StateFilter::Open),
            (false, "is", "closed") => self.state = Some(StateFilter::Closed),
            (false, "is", "issue") => self.kind = Some(KindFilter::Issue),
            (false, "is", "pr" | "pull") => self.kind = Some(KindFilter::Pull),
            (false, "is", "merged") => self.merged = true,
            (false, "is", "draft") => self.draft = true,
            (false, "is", "locked") => self.locked = true,
            (false, "author", _) => self.author = Some(Who::parse(value)),
            (false, "assignee", _) => self.assignee = Some(Who::parse(value)),
            (false, "no", "assignee") => self.no_assignee = true,
            (false, "no", "label") => self.no_label = true,
            (false, "no", "milestone") => self.no_milestone = true,
            (false, "label", _) => push_unique_ci(&mut self.labels, value),
            (true, "label", _) => push_unique_ci(&mut self.not_labels, value),
            (false, "milestone", _) => self.milestone = Some(value.to_owned()),
            (false, "sort", _) => match IssueSort::parse(value) {
                Some(sort) => self.sort = sort,
                None => self.ignored.push(term),
            },
            _ => self.ignored.push(term),
        }
    }

    /// Whether `label` is required (case-insensitively).
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }

    /// The same query with `state`.
    #[must_use]
    pub fn with_state(&self, state: StateFilter) -> Self {
        Self { state: Some(state), ..self.clone() }
    }

    /// The same query with `sort`.
    #[must_use]
    pub fn with_sort(&self, sort: IssueSort) -> Self {
        Self { sort, ..self.clone() }
    }

    /// The same query with `label` required, or no longer required if it already was.
    #[must_use]
    pub fn toggling_label(&self, label: &str) -> Self {
        let mut query = self.clone();
        if query.has_label(label) {
            query.labels.retain(|l| !l.eq_ignore_ascii_case(label));
        } else {
            query.labels.push(label.to_owned());
            query.no_label = false;
        }
        query
    }

    /// The same query by `author` (`None`: anyone).
    #[must_use]
    pub fn with_author(&self, author: Option<Who>) -> Self {
        Self { author, ..self.clone() }
    }

    /// The same query assigned to `assignee` (`None`: anyone).
    #[must_use]
    pub fn with_assignee(&self, assignee: Option<Who>) -> Self {
        Self { assignee, no_assignee: false, ..self.clone() }
    }

    /// The same query without any assignee.
    #[must_use]
    pub fn without_assignee(&self) -> Self {
        Self { assignee: None, no_assignee: true, ..self.clone() }
    }

    /// The same query in `milestone` (`None`: any).
    #[must_use]
    pub fn with_milestone(&self, milestone: Option<String>) -> Self {
        Self { milestone, no_milestone: false, ..self.clone() }
    }

    /// The same query without a milestone.
    #[must_use]
    pub fn without_milestone(&self) -> Self {
        Self { milestone: None, no_milestone: true, ..self.clone() }
    }
}

/// The canonical text: qualifiers in a fixed order, then the free text. Ignored terms are
/// dropped.
impl fmt::Display for IssueQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut terms: Vec<String> = Vec::new();
        match self.state {
            Some(StateFilter::Open) => terms.push("is:open".to_owned()),
            Some(StateFilter::Closed) => terms.push("is:closed".to_owned()),
            None => {}
        }
        match self.kind {
            Some(KindFilter::Issue) => terms.push("is:issue".to_owned()),
            Some(KindFilter::Pull) => terms.push("is:pr".to_owned()),
            None => {}
        }
        for (set, term) in [(self.merged, "is:merged"), (self.draft, "is:draft"), (self.locked, "is:locked")] {
            if set {
                terms.push(term.to_owned());
            }
        }
        if let Some(author) = &self.author {
            terms.push(format!("author:{author}"));
        }
        if let Some(assignee) = &self.assignee {
            terms.push(format!("assignee:{assignee}"));
        }
        if self.no_assignee {
            terms.push("no:assignee".to_owned());
        }
        terms.extend(self.labels.iter().map(|label| format!("label:{}", quote(label))));
        terms.extend(self.not_labels.iter().map(|label| format!("-label:{}", quote(label))));
        if self.no_label {
            terms.push("no:label".to_owned());
        }
        if let Some(milestone) = &self.milestone {
            terms.push(format!("milestone:{}", quote(milestone)));
        }
        if self.no_milestone {
            terms.push("no:milestone".to_owned());
        }
        if self.sort != IssueSort::default() {
            terms.push(format!("sort:{}", self.sort.as_str()));
        }
        terms.extend(self.text.iter().map(|word| quote(word)));
        f.write_str(&terms.join(" "))
    }
}

fn push_unique_ci(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|v| v.eq_ignore_ascii_case(value)) {
        values.push(value.to_owned());
    }
}

/// `value`, in double quotes if it contains whitespace or a quote (quotes inside are dropped).
fn quote(value: &str) -> String {
    if value.chars().any(|c| c.is_whitespace() || c == '"') { format!("\"{}\"", value.replace('"', "")) } else { value.to_owned() }
}

/// Splits on whitespace outside double quotes; quotes are removed (`label:"a b"` → `label:a b`).
fn split_terms(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in input.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    terms.push(std::mem::take(&mut current));
                }
                started = false;
            }
            c if c.is_control() => {}
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        terms.push(current);
    }
    terms.retain(|term| !term.is_empty() && term != "-");
    terms
}

#[cfg(test)]
mod tests;
