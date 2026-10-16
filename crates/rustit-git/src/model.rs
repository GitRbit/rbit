//! Plain-data views of Git objects, detached from gitoxide lifetimes so they can cross
//! thread boundaries and be handed straight to templates.

use rustit_core::Timestamp;

#[derive(Debug, Clone)]
pub struct CommitSummary {
    /// Full hexadecimal object id.
    pub id: String,
    /// Abbreviated id for display (7 characters).
    pub short_id: String,
    /// First line of the message.
    pub summary: String,
    /// Remainder of the message after the summary, if any.
    pub body: Option<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: Timestamp,
    pub parent_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    // Declaration order is display order: directories before files.
    Directory,
    Submodule,
    File,
    Executable,
    Symlink,
}

impl EntryKind {
    pub fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub kind: EntryKind,
    pub object_id: String,
}

/// The contents of a directory at a given revision.
#[derive(Debug, Clone)]
pub struct TreeView {
    /// The commit the revision resolved to.
    pub commit: CommitSummary,
    /// Sorted: directories first, then case-insensitive by name.
    pub entries: Vec<TreeEntry>,
}

/// A file at a given revision.
#[derive(Debug, Clone)]
pub struct BlobView {
    pub commit: CommitSummary,
    pub size: u64,
    /// `None` when the blob is larger than the render limit.
    pub data: Option<Vec<u8>>,
    pub is_binary: bool,
}

impl BlobView {
    /// The contents as UTF-8 text when it is renderable text.
    pub fn text(&self) -> Option<&str> {
        if self.is_binary { None } else { self.data.as_deref().and_then(|d| std::str::from_utf8(d).ok()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Tag,
}

#[derive(Debug, Clone)]
pub struct RefSummary {
    /// Short name, e.g. `main` or `v1.0.0`.
    pub name: String,
    pub kind: RefKind,
    pub target: String,
}

/// A branch or tag with the commit it points at (annotated tags are peeled).
#[derive(Debug, Clone)]
pub struct RefDetail {
    pub name: String,
    pub kind: RefKind,
    pub commit: CommitSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

/// What can be shown for one changed file.
#[derive(Debug, Clone)]
pub enum FileDiffBody {
    Text(Vec<crate::diff::Hunk>),
    Binary,
    /// One side exceeded the per-file size limit, or the commit's line budget ran out.
    TooLarge,
    /// Only the mode or path changed (e.g. made executable, renamed without edits).
    ModeOnly,
    /// A submodule pointer moved; ids are absent when it was added or removed.
    Submodule { old: Option<String>, new: Option<String> },
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    /// Source path for renames and copies.
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    pub additions: usize,
    pub deletions: usize,
    /// False when the file was skipped (too large, or the commit's line budget ran out),
    /// in which case `additions`/`deletions` are zero and must not be displayed.
    pub stats_known: bool,
    pub body: FileDiffBody,
}

/// A commit with everything it changed relative to its first parent.
#[derive(Debug, Clone)]
pub struct CommitDetail {
    pub commit: CommitSummary,
    pub committer_name: String,
    pub files: Vec<FileDiff>,
    /// Some files were omitted, or not diffed, because the commit is very large.
    pub truncated: bool,
}

impl CommitDetail {
    pub fn additions(&self) -> usize {
        self.files.iter().map(|f| f.additions).sum()
    }

    pub fn deletions(&self) -> usize {
        self.files.iter().map(|f| f.deletions).sum()
    }
}
