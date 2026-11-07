//! Typed workflow calls records; expression text remains unevaluated.
use std::collections::BTreeMap;

// Actions: reusable workflows.
/// `on.workflow_call`: what a calling job passes and gets back.
#[derive(Debug, Clone, Default)]
pub struct WorkflowCall {
    pub inputs: BTreeMap<String, CallInput>,
    /// `outputs.<name>.value`, as templates over the `jobs` context.
    pub outputs: BTreeMap<String, String>,
    /// Each declared secret, and whether it is required.
    pub secrets: BTreeMap<String, bool>,
}

/// One `on.workflow_call.inputs` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct CallInput {
    pub kind: CallInputKind,
    pub required: bool,
    /// `default`, as a value of `kind`.
    pub default: Option<serde_json::Value>,
}

/// The type of a `workflow_call` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallInputKind {
    Boolean,
    Number,
    String,
}

impl CallInputKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
        }
    }
}

/// A job that calls a reusable workflow (`jobs.<id>.uses`).
#[derive(Debug, Clone)]
pub struct JobCall {
    /// `uses:` as written.
    pub uses: String,
    pub target: CallTarget,
    /// `with:`, as templates.
    pub with: BTreeMap<String, String>,
    pub secrets: CallSecrets,
}

/// The workflow a job calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    /// `./path/to/workflow.yml`, in the repository and commit of the calling workflow.
    Local { path: String },
    /// `owner/repo/path/to/workflow.yml@ref`, a repository of this instance.
    Remote { owner: String, repo: String, path: String, reference: String },
}

/// `jobs.<id>.secrets`: `inherit`, or each secret of the called workflow by name with a
/// template over the caller's `secrets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallSecrets {
    Inherit,
    Map(BTreeMap<String, String>),
}

impl Default for CallSecrets {
    fn default() -> Self {
        Self::Map(BTreeMap::new())
    }
}
