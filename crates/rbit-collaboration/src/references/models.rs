//! Reference identities and ordered bounded scan results.
use super::find;
use crate::threads::IssueNumber;

/// At most this many distinct mentions of one text are honoured.
pub const MAX_MENTIONS: usize = 50;

/// Words that, before a `#N`, close issue `N`.
pub const CLOSING_KEYWORDS: [&str; 9] = ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"];

/// What a reference points at.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reference {
    /// `#12`: an issue or pull request of the same repository.
    Local(IssueNumber),
    /// `owner/repo#12`.
    Remote(RemoteRef),
    /// `@name`, as written (compare case-insensitively).
    Mention(String),
}

/// `owner/repo#12`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteRef {
    pub owner: String,
    pub repo: String,
    pub number: IssueNumber,
}

impl RemoteRef {
    /// Whether it names `owner/repo` (names compare case-insensitively).
    pub fn is_repository(&self, owner: &str, repo: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner) && self.repo.eq_ignore_ascii_case(repo)
    }
}

/// A reference found in a text: `text[start..end]` is exactly what was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    pub reference: Reference,
    /// A closing keyword precedes it (only for [`Reference::Local`]).
    pub closes: bool,
}

/// The distinct references of a whole text, in order of first appearance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub local: Vec<IssueNumber>,
    pub remote: Vec<RemoteRef>,
    /// Lower-cased, at most [`MAX_MENTIONS`].
    pub mentions: Vec<String>,
    /// Same-repository numbers after a closing keyword.
    pub closes: Vec<IssueNumber>,
}

impl Scan {
    fn add(&mut self, found: Found) {
        match found.reference {
            Reference::Local(number) => {
                push_unique(&mut self.local, number);
                if found.closes {
                    push_unique(&mut self.closes, number);
                }
            }
            Reference::Remote(remote) => push_unique(&mut self.remote, remote),
            Reference::Mention(name) => {
                let name = name.to_ascii_lowercase();
                if self.mentions.len() < MAX_MENTIONS {
                    push_unique(&mut self.mentions, name);
                }
            }
        }
    }

    /// Accumulates plain text from a Markdown/content adapter with the same bounded rules.
    pub fn add_text(&mut self, text: &str) {
        for found in find(text) {
            self.add(found);
        }
    }

    /// Merges `other` into this scan (for several texts of one thread).
    pub fn extend(&mut self, other: Self) {
        for number in other.local {
            push_unique(&mut self.local, number);
        }
        for remote in other.remote {
            push_unique(&mut self.remote, remote);
        }
        for name in other.mentions {
            if self.mentions.len() < MAX_MENTIONS {
                push_unique(&mut self.mentions, name);
            }
        }
        for number in other.closes {
            push_unique(&mut self.closes, number);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.remote.is_empty() && self.mentions.is_empty()
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
