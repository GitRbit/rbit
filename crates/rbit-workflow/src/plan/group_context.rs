//! Construct GitHub-compatible run grouping/event contexts without changing field values.
use super::contexts::RunContext;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// What the server knows of a run before it exists, as the triggers fill it in.
/// [`GroupContext::run_context`] turns it into the [`RunContext`] a run is planned with: the
/// `github` context a task gets later (§12.4), less what only the run has. `run_id` is a
/// placeholder (planning tells a group naming it apart by planning twice), `run_number` and
/// `run_attempt` are absent, and `github.event` carries the pull request number, the
/// dispatch inputs or the schedule — the task's context has the whole event payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupContext {
    /// The workflow's name, or its path when it has none.
    pub workflow: String,
    /// The workflow file's path (`.rbit/workflows/ci.yml`), for `github.workflow_ref`.
    pub workflow_path: String,
    /// The full ref (`refs/heads/main`, `refs/pull/7/head`).
    pub reference: String,
    /// Pull requests only: the head branch.
    pub head_ref: String,
    /// Pull requests only: the base branch.
    pub base_ref: String,
    pub event_name: String,
    /// `owner/name`.
    pub repository: String,
    pub run_id: i64,
    pub pull_request_number: Option<u64>,
    /// The commit the run is for.
    pub sha: String,
    /// Logins of the account that caused the run and of the one that triggered it; empty
    /// when nobody did (schedules).
    pub actor: String,
    pub triggering_actor: String,
    pub repository_owner: String,
    pub repository_id: i64,
    /// The instance's public URL, without a trailing slash.
    pub server_url: String,
    /// `schedule` runs only: the cron expression that fired.
    pub schedule: String,
    /// `workflow_dispatch` only: the checked inputs, typed.
    pub inputs: Map<String, Value>,
    /// Repository then owner variables; empty for untrusted runs.
    pub vars: BTreeMap<String, String>,
}

impl GroupContext {
    /// The values as a `github` context. Ids are strings, as the task's context has them.
    pub fn github(&self) -> Map<String, Value> {
        let reference = self.reference.as_str();
        let ref_name =
            ["refs/heads/", "refs/tags/", "refs/pull/"].iter().find_map(|prefix| reference.strip_prefix(prefix)).unwrap_or(reference);
        let mut event = Map::new();
        if let Some(number) = self.pull_request_number {
            event.insert("number".to_owned(), json!(number));
            event.insert("pull_request".to_owned(), json!({ "number": number }));
        }
        if !self.inputs.is_empty() {
            let as_text = self.inputs.iter().map(|(name, value)| (name.clone(), Value::String(crate::expr::to_text(value)))).collect();
            event.insert("inputs".to_owned(), Value::Object(as_text));
        }
        if !self.schedule.is_empty() {
            event.insert("schedule".to_owned(), json!(self.schedule));
        }
        let github = json!({
            "workflow": self.workflow,
            "workflow_ref": format!("{}/{}@{}", self.repository, self.workflow_path, self.reference),
            "ref": self.reference,
            "ref_name": ref_name,
            "ref_type": if reference.starts_with("refs/tags/") { "tag" } else { "branch" },
            "head_ref": self.head_ref,
            "base_ref": self.base_ref,
            "sha": self.sha,
            "event_name": self.event_name,
            "event": event,
            "repository": self.repository,
            "repository_owner": self.repository_owner,
            "repository_id": self.repository_id.to_string(),
            "actor": self.actor,
            "triggering_actor": self.triggering_actor,
            "run_id": self.run_id.to_string(),
            "server_url": self.server_url,
            "api_url": format!("{}/api/v1", self.server_url),
        });
        github.as_object().cloned().unwrap_or_default()
    }

    /// The [`RunContext`] a run is planned with: [`Self::github`], the inputs and variables.
    pub fn run_context(&self) -> RunContext {
        RunContext { github: self.github(), inputs: self.inputs.clone(), vars: self.vars.clone() }
    }
}
