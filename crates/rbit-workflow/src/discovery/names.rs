//! How a person names one workflow: its file name, or its path in a workflow directory.

use super::{WORKFLOW_DIRS, is_workflow_file_name};

/// The workflow a dispatch or a workflow page names: a file name (`ci.yml`, found in
/// whichever directory discovery reads), or a path inside one of [`WORKFLOW_DIRS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowFileName {
    /// The directory it was given with, if any.
    dir: Option<&'static str>,
    name: String,
}

impl WorkflowFileName {
    /// `given`, trimmed; `None` unless it names one workflow file: no other directory, no
    /// `.`/`..`, a `yml` or `yaml` extension.
    pub fn parse(given: &str) -> Option<Self> {
        let given = given.trim();
        let (dir, name) = match WORKFLOW_DIRS
            .iter()
            .find_map(|dir| given.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')).map(|rest| (*dir, rest)))
        {
            Some((dir, name)) => (Some(dir), name),
            None => (None, given),
        };
        let valid = !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".." && is_workflow_file_name(name);
        valid.then(|| Self { dir, name: name.to_owned() })
    }

    /// Whether the workflow file at `path` is the one named.
    pub fn matches(&self, path: &str) -> bool {
        match self.dir {
            Some(dir) => path.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')) == Some(self.name.as_str()),
            None => path.rsplit('/').next() == Some(self.name.as_str()),
        }
    }

    /// The name as given, for messages.
    pub fn shown(&self) -> String {
        match self.dir {
            Some(dir) => format!("{dir}/{}", self.name),
            None => self.name.clone(),
        }
    }
}
