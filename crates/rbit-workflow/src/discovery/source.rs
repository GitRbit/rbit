//! The port discovery reads a commit through, and the reads every caller shares.

use super::{Discovery, Listing, Selected, WorkflowFile};
use crate::error::{ErrorKind, WorkflowError};
use crate::limits::{Deadline, Limits};
use crate::model::Workflow;

/// A file that could not be read. The source records why (its own error stays its own);
/// the workflow error says only that the file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// One commit's files, as discovery needs them. Implemented over Git by the server; reads
/// may block, so callers run discovery off the request path.
pub trait WorkflowSource {
    /// Why a directory could not be listed: discovery stops and returns it.
    type Error;

    /// The entries of `dir`, or [`Listing::Missing`] when the commit has no such directory.
    fn list(&mut self, dir: &'static str) -> Result<Listing, Self::Error>;

    /// The bytes of the file at `path`, or `None` when it is larger than `max_bytes`.
    fn read(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Unreadable>;

    /// Told when the directory has more workflow files than are read.
    fn truncated(&mut self, _selected: &Selected) {}
}

/// The paths of the commit's workflow files (see the module documentation). Nothing is read
/// or parsed.
pub fn paths<S: WorkflowSource + ?Sized>(source: &mut S) -> Result<Vec<String>, S::Error> {
    let mut discovery = Discovery::new();
    while let Some(dir) = discovery.next_dir() {
        let listing = source.list(dir)?;
        if let Some(selected) = discovery.record(listing) {
            if selected.truncated() {
                source.truncated(&selected);
            }
            return Ok(selected.paths);
        }
    }
    Ok(Vec::new())
}

/// The workflow at `path`, read within `limits.max_bytes` and parsed. The deadline is
/// checked first; a file that cannot be read or is too large is a workflow error.
pub fn load<S: WorkflowSource + ?Sized>(
    source: &mut S,
    path: &str,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Workflow, WorkflowError> {
    deadline.check()?;
    let data =
        source.read(path, limits.max_bytes).map_err(|Unreadable| WorkflowError::new(ErrorKind::Parse, "the file could not be read"))?;
    let Some(data) = data else {
        return Err(WorkflowError::limit(format!("the file is larger than {} bytes", limits.max_bytes), None));
    };
    crate::parse(&data, limits, deadline)
}

/// Every workflow file of the commit, each read and parsed.
pub fn discover<S: WorkflowSource + ?Sized>(source: &mut S, limits: &Limits, deadline: &Deadline) -> Result<Vec<WorkflowFile>, S::Error> {
    Ok(paths(source)?
        .into_iter()
        .map(|path| {
            let parsed = load(source, &path, limits, deadline);
            WorkflowFile { path, parsed }
        })
        .collect())
}

/// The first workflow file whose path `wanted` accepts, read and parsed, among those
/// [`discover`] reads; the others are not read.
pub fn discover_one<S: WorkflowSource + ?Sized>(
    source: &mut S,
    wanted: impl Fn(&str) -> bool,
    limits: &Limits,
    deadline: &Deadline,
) -> Result<Option<WorkflowFile>, S::Error> {
    Ok(paths(source)?.into_iter().find(|path| wanted(path)).map(|path| {
        let parsed = load(source, &path, limits, deadline);
        WorkflowFile { path, parsed }
    }))
}
