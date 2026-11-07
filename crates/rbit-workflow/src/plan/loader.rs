//! Reusable-workflow loader contract and original nesting/name-count bounds.
use crate::model::{CallTarget, Workflow};

// Actions: reusable workflows.
/// How many levels of workflows may call each other, the run's own included (GitHub's limit).
pub const MAX_CALL_DEPTH: usize = 10;

/// How many different workflows one run may call (GitHub's limit).
pub const MAX_CALLED_WORKFLOWS: usize = 50;

/// Finds the workflows that jobs call.
pub trait Loader {
    /// The workflow `target` names, called from the workflow that [`Loaded::origin`] named
    /// (`None`: the run's own). The error says why it cannot be used, without telling a
    /// private repository from a missing one.
    fn load(&mut self, origin: Option<&str>, target: &CallTarget) -> Result<Loaded, String>;
}

/// A called workflow.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub workflow: Workflow,
    /// Where its own `./` calls are found: opaque to planning, given back to [`Loader::load`].
    pub origin: String,
    /// The file at its commit, to count different workflows.
    pub id: String,
}

/// The [`Loader`] of a workflow that may call none: every call is refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCalls;

impl Loader for NoCalls {
    fn load(&mut self, _origin: Option<&str>, _target: &CallTarget) -> Result<Loaded, String> {
        Err("calling a workflow needs the rbit server".to_owned())
    }
}
