//! What a step does: a `run:` script ([`run`]) or a built-in action ([`checkout`], …).
//!
//! A built-in's `with:` inputs are evaluated at step time, filled with the built-in's
//! defaults and checked against what the input takes (`rbit_workflow::builtins`), so an
//! expression that evaluates to `maybe` for a boolean fails the step with a message.

pub mod cache;
pub mod checkout;
pub mod run;

use std::{collections::BTreeMap, time::Duration};

use rbit_workflow::{
    builtins,
    protocol::StepStatus,
    spec::{Builtin, BuiltinKind},
};

use crate::job::Job;

/// How a step ended, and the outputs it set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub status: StepStatus,
    pub outputs: BTreeMap<String, String>,
}

impl Outcome {
    pub fn success(outputs: BTreeMap<String, String>) -> Self {
        Self { status: StepStatus::Success, outputs }
    }

    pub fn failure() -> Self {
        Self { status: StepStatus::Failure, outputs: BTreeMap::new() }
    }

    pub fn cancelled() -> Self {
        Self { status: StepStatus::Cancelled, outputs: BTreeMap::new() }
    }
}

/// A built-in's inputs after evaluation: every input it has, with its default where the
/// step set none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inputs(BTreeMap<&'static str, String>);

impl Inputs {
    /// The input's value; empty when it is unset and has no default.
    pub fn get(&self, name: &str) -> &str {
        self.0.get(name).map_or("", String::as_str)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.get(name) == "true"
    }

    /// Evaluates `builtin.with` for `job`.
    pub fn evaluate(job: &Job, builtin: &Builtin) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for input in builtins::inputs(builtin.kind) {
            let value = match builtin.with.get(input.name) {
                Some(template) => job.render(template).map_err(|error| format!("`with.{}`: {error}", input.name))?,
                None => input.default.unwrap_or_default().to_owned(),
            };
            if input.required && value.trim().is_empty() {
                return Err(format!("`{}` needs `with.{}`", builtin.uses, input.name));
            }
            if !value.is_empty() || input.default.is_some() {
                builtins::check_input(builtin.kind, &builtin.uses, input.name, &value)?;
            }
            values.insert(input.name, value);
        }
        Ok(Self(values))
    }
}

/// Runs a built-in action.
pub async fn builtin(job: &mut Job, builtin: &Builtin, timeout: Option<Duration>) -> Outcome {
    job.reporter.log(&format!("##[group]Run {}", builtin.uses));
    for (name, value) in &builtin.with {
        job.reporter.log(&format!("  {name}: {value}"));
    }
    job.reporter.log("##[endgroup]");
    let inputs = match Inputs::evaluate(job, builtin) {
        Ok(inputs) => inputs,
        Err(error) => return job.fail(&error),
    };
    match builtin.kind {
        BuiltinKind::Checkout => checkout::checkout(job, &inputs, timeout).await,
        BuiltinKind::UploadArtifact | BuiltinKind::DownloadArtifact => {
            job.fail(&format!("`{}`: artifacts are not available in rbit Actions yet", builtin.uses))
        }
        BuiltinKind::Cache | BuiltinKind::CacheRestore | BuiltinKind::CacheSave => cache::cache(job, builtin.kind, &inputs).await,
    }
}
