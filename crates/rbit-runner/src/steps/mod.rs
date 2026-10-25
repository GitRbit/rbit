//! What a step does: a `run:` script ([`run`]) or a built-in action.

pub mod run;

use std::collections::BTreeMap;

use rbit_workflow::{
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

/// Runs a built-in action.
pub fn builtin(job: &Job, builtin: &Builtin) -> Outcome {
    job.reporter.log(&format!("##[group]Run {}", builtin.uses));
    job.reporter.log("##[endgroup]");
    match builtin.kind {
        BuiltinKind::UploadArtifact | BuiltinKind::DownloadArtifact => {
            job.fail(&format!("`{}`: artifacts are not available in rbit Actions yet", builtin.uses))
        }
        BuiltinKind::Checkout | BuiltinKind::Cache | BuiltinKind::CacheRestore | BuiltinKind::CacheSave => {
            job.fail(&format!("`{}` is not available in this build of rbit-runner yet", builtin.uses))
        }
    }
}
