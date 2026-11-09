//! Import options, status, phases and counts as stored, and the text rules every
//! imported item follows (re-exported unchanged by `rbit_core::import`).
use std::fmt;

use serde::{Deserialize, Serialize};

/// Warnings kept per import (the rest are counted, not stored).
pub const MAX_IMPORT_WARNINGS: usize = 50;
/// Longest warning or error message kept.
pub const MAX_IMPORT_MESSAGE_CHARS: usize = 500;

/// What an import brings along besides the Git repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "one checkbox each on the import form")]
pub struct ImportOptions {
    /// Issues and pull requests with their comments (pull request heads are fetched as
    /// `refs/pull/{n}/head`).
    pub issues: bool,
    pub labels: bool,
    pub milestones: bool,
    /// Releases with their attached files.
    pub releases: bool,
    /// Git LFS objects referenced by the repository's history.
    pub lfs: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { issues: true, labels: true, milestones: true, releases: true, lfs: true }
    }
}

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl ImportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Queued, Self::Running, Self::Succeeded, Self::Failed].into_iter().find(|status| status.as_str() == value)
    }

    /// Queued or running.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// The step an import is working on (or stopped at).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImportPhase {
    Repository,
    Lfs,
    Labels,
    Milestones,
    Issues,
    Releases,
    Finished,
}

impl ImportPhase {
    pub const ALL: [Self; 7] = [Self::Repository, Self::Lfs, Self::Labels, Self::Milestones, Self::Issues, Self::Releases, Self::Finished];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Lfs => "lfs",
            Self::Labels => "labels",
            Self::Milestones => "milestones",
            Self::Issues => "issues",
            Self::Releases => "releases",
            Self::Finished => "finished",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|phase| phase.as_str() == value)
    }

    /// Shown on the progress page.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Repository => "Copying the Git repository",
            Self::Lfs => "Copying Git LFS objects",
            Self::Labels => "Importing labels",
            Self::Milestones => "Importing milestones",
            Self::Issues => "Importing issues and pull requests",
            Self::Releases => "Importing releases",
            Self::Finished => "Finished",
        }
    }
}

/// How much an import has brought over so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportCounts {
    pub labels: u64,
    pub milestones: u64,
    pub issues: u64,
    pub pulls: u64,
    pub comments: u64,
    pub releases: u64,
    pub assets: u64,
    pub lfs_objects: u64,
    /// Warnings beyond [`MAX_IMPORT_WARNINGS`] that were not stored.
    pub dropped_warnings: u64,
}

/// A label colour as `#rrggbb` in lower case, from `rrggbb` (GitHub) or `#rrggbb` (Gitea).
pub fn label_color(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// The longest prefix of `text` that fits in `max_bytes` without splitting a character.
pub fn truncate_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The first `max_chars` characters of `text`.
pub fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices().nth(max_chars).map_or(text, |(index, _)| &text[..index])
}

/// A message trimmed to [`MAX_IMPORT_MESSAGE_CHARS`] without control characters other than
/// newlines, for warnings and errors shown on the progress page.
pub fn clean_message(message: &str) -> String {
    message.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_IMPORT_MESSAGE_CHARS).collect()
}

impl fmt::Display for ImportPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
