//! Rendering references preserve stable ordering and stored account spelling.
use crate::{
    IssueState,
    threads::{IssueKind, IssueNumber},
};
use std::collections::BTreeMap;

/// An existing thread a `#N` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedIssue {
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// What the references in a batch of texts resolve to: the existing issues of the
/// repository among `#N` and the existing accounts among `@name`. The default resolves
/// nothing (every reference renders as plain text).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    /// Existing threads of the repository, by number.
    pub issues: BTreeMap<IssueNumber, ReferencedIssue>,
    /// Existing accounts: lower-cased name → the name as stored.
    pub accounts: BTreeMap<String, String>,
}

impl ReferenceSet {
    pub fn issue(&self, number: IssueNumber) -> Option<&ReferencedIssue> {
        self.issues.get(&number)
    }

    /// The stored spelling of account `name`, if it exists.
    pub fn account(&self, name: &str) -> Option<&str> {
        self.accounts.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}
