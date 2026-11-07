//! Owned run inputs and borrowed per-job matrix/strategy context lookup.
use crate::expr::{ContextMap, Contexts};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What the server knows of a run when it plans it: the contexts of §12.4's first row
/// besides `matrix` and `strategy`, which planning works out itself. The field types are
/// those of [`crate::spec::Contexts`], so the same values go to the runner later.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunContext {
    /// The `github` context, `event` included and the token never.
    pub github: Map<String, Value>,
    /// `inputs` of a `workflow_dispatch` run; empty otherwise.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables; empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
}

impl RunContext {
    /// `github`, `inputs` and `vars`, for [`crate::expr`].
    pub fn contexts(&self) -> ContextMap {
        let vars: Map<String, Value> = self.vars.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
        ContextMap::new().with("github", self.github.clone()).with("inputs", self.inputs.clone()).with("vars", vars)
    }
}

/// The run's contexts plus one job's `matrix` and `strategy`, without copying the former.
pub(super) struct JobContexts<'a> {
    pub(super) run: &'a ContextMap,
    pub(super) matrix: Value,
    pub(super) strategy: Value,
}

impl Contexts for JobContexts<'_> {
    fn context(&self, name: &str) -> Option<&Value> {
        match name {
            "matrix" => Some(&self.matrix),
            "strategy" => Some(&self.strategy),
            _ => self.run.context(name),
        }
    }
}
