//! Which workflow files a commit has, and how one is read: the same precedence, file
//! filter, count cap, size budget and error text for every event, page and called workflow.
//!
//! * Workflows live in the first of [`WORKFLOW_DIRS`] that exists; the others are never
//!   read, even when the first holds no workflow files.
//! * In it, the regular and executable files whose extension is `yml` or `yaml` (by
//!   [`std::path::Path::extension`], so `.yml` alone and `CI.YML` are not workflows), by
//!   name, at most [`MAX_WORKFLOW_FILES`]. Subdirectories, links and submodules are not.
//! * A file is read within [`crate::Limits::max_bytes`] after the [`crate::Deadline`] is
//!   checked; one that cannot be read or is too large is a workflow error, not a Git error.
//! * A workflow is called by its `name`, or by its path when the name is blank or the file
//!   does not parse; a problem is shown after its path ([`describe`]).
//!
//! No Git here: callers list directories and read files through a [`WorkflowSource`]
//! ([`source`]), or drive the precedence themselves with [`Discovery`] when their reads
//! are asynchronous. [`evaluate`] then decides what each file makes of an event.

mod names;
mod outcomes;
mod source;
#[cfg(test)]
mod tests;

use std::path::Path;

use crate::error::WorkflowError;
use crate::model::Workflow;

pub use self::names::WorkflowFileName;
pub use self::outcomes::{Evaluated, evaluate};
pub use self::source::{Unreadable, WorkflowSource, discover, discover_one, load, paths};

/// Where workflows live, in precedence order: only the first that exists is read.
pub const WORKFLOW_DIRS: [&str; 3] = [".rbit/workflows", ".forgejo/workflows", ".github/workflows"];
/// Most workflow files read from the directory.
pub const MAX_WORKFLOW_FILES: usize = 64;

/// One entry of a workflow directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    /// A regular or executable file: not a directory, link or submodule.
    pub is_file: bool,
}

/// A workflow directory as the commit has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// The commit has no such directory: the next one is looked at.
    Missing,
    /// The directory's entries: it decides discovery, whatever they are.
    Entries(Vec<DirEntry>),
}

/// The workflow files discovery settled on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The directory they are in.
    pub dir: &'static str,
    /// Their paths (`.rbit/workflows/ci.yml`), by name, at most [`MAX_WORKFLOW_FILES`].
    pub paths: Vec<String>,
    /// How many workflow files the directory has: more than `paths` when capped.
    pub available: usize,
}

impl Selected {
    /// Whether files beyond [`MAX_WORKFLOW_FILES`] were left out.
    pub fn truncated(&self) -> bool {
        self.available > self.paths.len()
    }
}

/// The walk over [`WORKFLOW_DIRS`]: list [`Discovery::next_dir`], hand its listing to
/// [`Discovery::record`], until it settles. Done when `next_dir` is `None`.
#[derive(Debug, Clone, Default)]
pub struct Discovery {
    next: usize,
}

impl Discovery {
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    /// The directory to list next; `None` once discovery is settled or every directory
    /// was missing.
    pub fn next_dir(&self) -> Option<&'static str> {
        WORKFLOW_DIRS.get(self.next).copied()
    }

    /// Takes the listing of [`Self::next_dir`]: `Some` once a directory exists.
    pub fn record(&mut self, listing: Listing) -> Option<Selected> {
        let dir = self.next_dir()?;
        let Listing::Entries(entries) = listing else {
            self.next += 1;
            return None;
        };
        self.next = WORKFLOW_DIRS.len();
        let mut names: Vec<String> =
            entries.into_iter().filter(|entry| entry.is_file && is_workflow_file_name(&entry.name)).map(|entry| entry.name).collect();
        names.sort();
        let available = names.len();
        names.truncate(MAX_WORKFLOW_FILES);
        Some(Selected { dir, paths: names.into_iter().map(|name| format!("{dir}/{name}")).collect(), available })
    }
}

/// Whether a file called `name` holds a workflow: its extension is `yml` or `yaml`.
pub fn is_workflow_file_name(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|extension| extension == "yml" || extension == "yaml")
}

/// A workflow file read at a commit.
#[derive(Debug)]
pub struct WorkflowFile {
    /// `.forgejo/workflows/ci.yml`.
    pub path: String,
    pub parsed: Result<Workflow, WorkflowError>,
}

impl WorkflowFile {
    /// The workflow's `name`, or its path when it has none (or does not parse).
    pub fn name(&self) -> String {
        match &self.parsed {
            Ok(workflow) => workflow.name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| self.path.clone()),
            Err(_) => self.path.clone(),
        }
    }
}

/// A workflow error as a run or a person sees it: the file, then the problem and where it is.
pub fn describe(path: &str, error: &WorkflowError) -> String {
    format!("{path}: {error}")
}
