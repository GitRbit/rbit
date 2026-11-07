//! Permission and concurrency declarations with original evaluation scopes.
use super::expressions::{Scope, flag, template};
use super::syntax::{check_keys, error};
use crate::error::WorkflowError;
use crate::model::{Concurrency, Permission, Permissions};
use crate::yaml::{Node, Value};

/// Every permission GitHub documents. rbit honours `contents`, `issues` and
/// `pull-requests` (§11.4) and accepts the others without effect.
pub(super) const GITHUB_PERMISSIONS: [&str; 15] = [
    "actions",
    "attestations",
    "checks",
    "contents",
    "deployments",
    "discussions",
    "id-token",
    "issues",
    "models",
    "packages",
    "pages",
    "pull-requests",
    "repository-projects",
    "security-events",
    "statuses",
];

pub(super) fn parse_permissions(node: &Node, path: &str) -> Result<Permissions, WorkflowError> {
    match &node.value {
        Value::Scalar(scalar) => match scalar.text().as_str() {
            "read-all" => Ok(Permissions::uniform(Permission::Read)),
            "write-all" => Ok(Permissions::uniform(Permission::Write)),
            _ => Err(error(format!("`{path}` must be read-all, write-all or a mapping"), node.at)),
        },
        Value::Mapping(scopes) => {
            let mut permissions = Permissions::uniform(Permission::None);
            for (key, value) in scopes.iter() {
                let level = match value.as_text().as_deref() {
                    Some("read") => Permission::Read,
                    Some("write") => Permission::Write,
                    Some("none") => Permission::None,
                    _ => return Err(error(format!("`{path}.{}` must be read, write or none", key.name), value.at)),
                };
                match key.name.as_str() {
                    "contents" => permissions.contents = level,
                    "issues" => permissions.issues = level,
                    "pull-requests" => permissions.pull_requests = level,
                    other if GITHUB_PERMISSIONS.contains(&other) => {}
                    other => return Err(error(format!("`{path}`: `{other}` is not a permission"), key.at)),
                }
            }
            Ok(permissions)
        }
        Value::Sequence(_) => Err(error(format!("`{path}` must be read-all, write-all or a mapping"), node.at)),
    }
}

/// Workflow-level (`scope` [`Scope::Run`]) or job-level ([`Scope::Job`]) `concurrency`.
pub(super) fn parse_concurrency(node: &Node, path: &str, scope: Scope) -> Result<Concurrency, WorkflowError> {
    let concurrency = match &node.value {
        Value::Scalar(_) => {
            let group = node.as_text().ok_or_else(|| error(format!("`{path}` must not be empty"), node.at))?;
            Concurrency { group, cancel_in_progress: "false".into(), at: node.at }
        }
        Value::Mapping(config) => {
            check_keys(config, path, &["group", "cancel-in-progress"], &[])?;
            let group = config
                .get("group")
                .and_then(Node::as_text)
                .filter(|group| !group.is_empty())
                .ok_or_else(|| error(format!("`{path}` needs a `group`"), node.at))?;
            let cancel_in_progress = match config.get("cancel-in-progress") {
                None => "false".into(),
                Some(value) => flag(value, &format!("{path}.cancel-in-progress"), scope)?,
            };
            Concurrency { group, cancel_in_progress, at: node.at }
        }
        Value::Sequence(_) => return Err(error(format!("`{path}` must be a group name or a mapping"), node.at)),
    };
    template(&concurrency.group, scope, &format!("{path}.group"), concurrency.at)?;
    Ok(concurrency)
}
