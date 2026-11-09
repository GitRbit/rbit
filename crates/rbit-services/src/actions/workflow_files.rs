//! The Git adapter of workflow discovery: [`WorkflowSource`] over a repository reader at
//! one commit. Precedence, filters, caps, read budgets and error text are decided in
//! [`rbit_workflow::discovery`]; this file only lists trees and reads blobs.

use rbit_git::{EntryKind, GitError, RepositoryReader, TreeView};
use rbit_workflow::{
    Deadline, Limits, Workflow, WorkflowError,
    discovery::{self, DirEntry, Listing, MAX_WORKFLOW_FILES, Selected, Unreadable, WorkflowSource},
};
use tokio::runtime::Handle;

/// A repository's workflow files at commit `sha`. Blocking: runs inside an evaluation.
pub(super) struct GitWorkflows<'a> {
    handle: &'a Handle,
    reader: &'a RepositoryReader,
    sha: &'a str,
}

impl<'a> GitWorkflows<'a> {
    pub(super) fn new(handle: &'a Handle, reader: &'a RepositoryReader, sha: &'a str) -> Self {
        Self { handle, reader, sha }
    }

    /// The workflow at `path`, read and parsed by [`discovery::load`].
    pub(super) fn load(mut self, path: &str, limits: &Limits, deadline: &Deadline) -> Result<Workflow, WorkflowError> {
        discovery::load(&mut self, path, limits, deadline)
    }
}

impl WorkflowSource for GitWorkflows<'_> {
    type Error = GitError;

    fn list(&mut self, dir: &'static str) -> Result<Listing, GitError> {
        listing(self.handle.block_on(self.reader.tree(self.sha.to_owned(), dir.to_owned())))
    }

    fn read(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Unreadable> {
        let blob = self.handle.block_on(self.reader.blob(self.sha.to_owned(), path.to_owned(), max_bytes)).map_err(|error| {
            tracing::warn!(%error, path, "cannot read a workflow file");
            Unreadable
        })?;
        Ok(blob.data)
    }

    fn truncated(&mut self, selected: &Selected) {
        tracing::warn!(dir = selected.dir, files = selected.available, "only the first {MAX_WORKFLOW_FILES} workflow files are read");
    }
}

/// A workflow directory's tree as discovery takes it: a missing directory is
/// [`Listing::Missing`], any other Git error stays an error.
pub(super) fn listing(tree: Result<TreeView, GitError>) -> Result<Listing, GitError> {
    match tree {
        Ok(tree) => Ok(Listing::Entries(
            tree.entries
                .into_iter()
                .map(|entry| DirEntry { is_file: matches!(entry.kind, EntryKind::File | EntryKind::Executable), name: entry.name })
                .collect(),
        )),
        Err(GitError::NotFound(_)) => Ok(Listing::Missing),
        Err(error) => Err(error),
    }
}
