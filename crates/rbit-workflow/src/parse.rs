//! Workflow file Ã¢â€ â€™ [`Workflow`], with every problem reported at its place in the file.
//!
//! Every key of Ã‚Â§12.2 is read into the typed model, and every other key is an error at
//! every level, because a typo must not silently change what a job does. GitHub keys rbit
//! lacks (container `ports`, runner groups, `cancel-timeout-minutes`, Ã¢â‚¬Â¦) say "not supported
//! by rbit".
//!
//! Structural checks: the shape of `on`, filters that compile, job ids, `runs-on`, `needs`
//! that name existing jobs without a cycle, a matrix that is a mapping of lists,
//! `permissions`, `concurrency`, containers, services, and steps with exactly one of `run`
//! and `uses`. Every `${{ }}` is parsed here, and may only use the contexts its field is
//! evaluated with (Ã‚Â§12.4): `run-name` and `strategy` see `github`, `inputs` and `vars`; the
//! fields the server evaluates per job also `matrix` and `strategy`; a job's `if` also
//! `needs`; what the runner evaluates sees everything. Status functions only work in `if`
//! and `hashFiles` only on the runner. Expressions are left as written; planning evaluates
//! the server's and the runner evaluates the rest.

mod calls;
mod dependencies;
mod dispatch;
mod environment;
mod expressions;
mod jobs;
mod permissions;
mod runtime;
mod steps;
mod strategy;
mod syntax;
mod triggers;

pub use calls::call_input_value;
pub use dispatch::MAX_DISPATCH_INPUTS;
pub(crate) use environment::parse_env;
pub use jobs::MAX_JOB_ID_BYTES;
pub(crate) use jobs::valid_job_id;
pub(crate) use steps::parse_steps;

use self::environment::parse_defaults;
use self::expressions::{Scope, template};
use self::jobs::parse_jobs;
use self::permissions::{parse_concurrency, parse_permissions};
use self::syntax::{check_keys, optional_text, text_field};
use self::triggers::parse_on;
use crate::error::{ErrorKind, Location, WorkflowError};
use crate::limits::{Deadline, Limits};
use crate::model::{CallTarget, Workflow};
use crate::yaml;
use crate::yaml::Value;

/// The keys of a workflow.
const WORKFLOW_KEYS: [&str; 8] = ["name", "run-name", "on", "env", "defaults", "permissions", "concurrency", "jobs"];

/// Parses and validates one workflow file.
pub fn parse(source: &[u8], limits: &Limits, deadline: &Deadline) -> Result<Workflow, WorkflowError> {
    if source.len() > limits.max_bytes {
        return Err(WorkflowError::limit(format!("the workflow file is larger than {} bytes", limits.max_bytes), None));
    }
    let source = std::str::from_utf8(source).map_err(|_| WorkflowError::new(ErrorKind::Parse, "the workflow file is not UTF-8"))?;
    let document = yaml::parse_document(source, limits)?;
    deadline.check()?;
    let Value::Mapping(document) = document.value else {
        return Err(WorkflowError::parse("a workflow must be a mapping with `on` and `jobs`", document.at));
    };
    check_keys(&document, "", &WORKFLOW_KEYS, &[])?;
    let name = optional_text(&document, "name")?;
    let run_name = text_field(&document, "run-name", "run-name")?;
    if let (Some(run_name), Some(node)) = (&run_name, document.get("run-name")) {
        template(run_name, Scope::Run, "run-name", node.at)?;
    }
    let on = match document.entry("on") {
        Some((_, node)) => parse_on(node)?,
        None => return Err(WorkflowError::parse("the workflow has no `on` section", Location::new(1, 1))),
    };
    let env = document.get("env").map(|node| parse_env(node, "env")).transpose()?.unwrap_or_default();
    let defaults = document.get("defaults").map(|node| parse_defaults(node, "defaults")).transpose()?.unwrap_or_default();
    let permissions = document.get("permissions").map(|node| parse_permissions(node, "permissions")).transpose()?;
    let concurrency = document.get("concurrency").map(|node| parse_concurrency(node, "concurrency", Scope::Run)).transpose()?;
    let Some((jobs_key, jobs_node)) = document.entry("jobs") else {
        return Err(WorkflowError::parse("the workflow has no `jobs` section", Location::new(1, 1)));
    };
    let jobs = parse_jobs(jobs_key, jobs_node, limits, deadline)?;
    Ok(Workflow { name, run_name, on, env, defaults, permissions, concurrency, jobs })
}

/// The workflow `uses` names, or `None` when it has neither form.
pub(crate) fn call_target(uses: &str) -> Option<CallTarget> {
    // Lowercase only, as GitHub: `.github/workflows/ci.YML` is no workflow there.
    #[expect(clippy::case_sensitive_file_extension_comparisons, reason = "GitHub reads only lowercase extensions")]
    let workflow_file = |path: &str| {
        (path.ends_with(".yml") || path.ends_with(".yaml"))
            && path.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'))
    };
    if let Some(path) = uses.strip_prefix("./") {
        return workflow_file(path).then(|| CallTarget::Local { path: path.to_owned() });
    }
    let (spec, reference) = uses.rsplit_once('@')?;
    let mut parts = spec.splitn(3, '/');
    let (owner, repo, path) = (parts.next()?, parts.next()?, parts.next()?);
    let name = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    (name(owner) && name(repo) && workflow_file(path) && !reference.is_empty() && !reference.contains(char::is_whitespace)).then(|| {
        CallTarget::Remote { owner: owner.to_owned(), repo: repo.to_owned(), path: path.to_owned(), reference: reference.to_owned() }
    })
}

#[cfg(test)]
mod tests;
