//! Webhook event envelopes: how each delivery body is assembled from shared objects.
//!
//! The objects inside (accounts, repositories, releases, commits, hooks) are GitHub-shaped
//! JSON the REST API also returns; composition renders them and hands them in. This
//! module owns what only webhooks send: the envelopes, the `push` contract (null ids,
//! compare links, commit order and cap, head commit, pusher), `ping` and which events
//! carry an `action`.
use serde_json::{Value, json};

/// The all-zero object id GitHub sends for a ref's missing side.
pub const NULL_SHA: &str = "0000000000000000000000000000000000000000";

/// Commits listed in a `push` payload; `total_commits` says how many there were.
pub const MAX_PUSH_COMMITS: usize = 20;

/// Events whose deliveries record the body's `action`; every other event has none.
const ACTION_EVENTS: [&str; 9] =
    ["repository", "release", "issues", "issue_comment", "pull_request", "pull_request_review", "member", "workflow_run", "workflow_job"];

/// Short, friendly sentences for `ping` payloads, in the spirit of GitHub's.
const ZEN: &[&str] = &[
    "Keep it logically awesome.",
    "Simplicity is the ultimate sophistication.",
    "Small commits, clear messages.",
    "Practicality beats purity.",
    "Design for failure.",
    "Speak like a human.",
];

/// Adds the `organization` field for organisation-owned repositories.
pub fn with_organization(mut payload: Value, organization: Option<Value>) -> Value {
    if let Some(org) = organization {
        payload["organization"] = org;
    }
    payload
}

/// A `repository` payload (`created`, `deleted`, `transferred`, `archived` or
/// `unarchived`).
pub fn repository_event(action: &str, repository: &Value, sender: Option<&Value>) -> Value {
    json!({
        "action": action,
        "repository": repository,
        "sender": sender,
    })
}

/// The `repository` object of a deleted repository: the row is gone, so only what the
/// event still carries.
pub fn deleted_repository(id: i64, full_name: &str, owner: Option<&Value>, base_url: &str) -> Value {
    let name = full_name.rsplit_once('/').map_or(full_name, |(_, name)| name);
    json!({
        "id": id,
        "name": name,
        "full_name": full_name,
        "owner": owner,
        "html_url": format!("{base_url}/{full_name}"),
    })
}

/// A `repository` payload with `action: transferred`, naming the previous owner (`from`,
/// an organisation when `from_organization`).
pub fn transferred(
    repository: &Value,
    from: &Value,
    from_organization: bool,
    sender: Option<&Value>,
    organization: Option<Value>,
) -> Value {
    let key = if from_organization { "organization" } else { "user" };
    let mut payload = repository_event("transferred", repository, sender);
    payload["changes"] = json!({ "owner": { "from": { key: from } } });
    with_organization(payload, organization)
}

/// A `fork` payload: `forkee` is the new repository, `repository` the one forked.
pub fn fork(forkee: &Value, repository: &Value, sender: Option<&Value>, organization: Option<Value>) -> Value {
    let payload = json!({
        "forkee": forkee,
        "repository": repository,
        "sender": sender,
    });
    with_organization(payload, organization)
}

/// A `release` payload (`action: published`).
pub fn release_published(release: &Value, repository: &Value, sender: Option<&Value>, organization: Option<Value>) -> Value {
    let payload = json!({
        "action": "published",
        "release": release,
        "repository": repository,
        "sender": sender,
    });
    with_organization(payload, organization)
}

/// A `member` payload: `action` is `added`, `edited` or `removed`; `changes` carries
/// GitHub's `permission` (`to` when added, `from` and `to` when edited, plus the older
/// `old_permission.from`).
pub fn member(
    action: &str,
    permission: (Option<&str>, Option<&str>),
    member: &Value,
    repository: &Value,
    sender: Option<&Value>,
    organization: Option<Value>,
) -> Value {
    let mut payload = json!({
        "action": action,
        "member": member,
        "repository": repository,
        "sender": sender,
    });
    match permission {
        (None, Some(to)) => payload["changes"] = json!({ "permission": { "to": to } }),
        (Some(from), Some(to)) => {
            payload["changes"] = json!({ "permission": { "from": from, "to": to }, "old_permission": { "from": from } });
        }
        _ => {}
    }
    with_organization(payload, organization)
}

/// A `ping` payload for a newly created (or explicitly pinged) hook; `context` names
/// what the hook is on (`repository` or `organization`).
pub fn ping(hook_json: &Value, hook_id: i64, context: Option<(&str, Value)>, sender: Option<&Value>) -> Value {
    let zen = ZEN[usize::try_from(hook_id.unsigned_abs()).unwrap_or_default() % ZEN.len()];
    let mut payload = json!({
        "zen": zen,
        "hook_id": hook_id,
        "hook": hook_json,
        "sender": sender,
    });
    if let Some((key, value)) = context {
        payload[key] = value;
    }
    payload
}

/// The `action` stored with a delivery of `event` (its wire name), read from its payload.
pub fn action_of(event: &str, payload: &Value) -> Option<String> {
    if ACTION_EVENTS.contains(&event) { payload.get("action").and_then(Value::as_str).map(str::to_owned) } else { None }
}

/// One ref update of a push. `None` sides are a created or deleted ref.
#[derive(Debug, Clone, Copy)]
pub struct PushedRef<'a> {
    pub name: &'a str,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
    pub created: bool,
    pub deleted: bool,
}

/// A `push` payload for one ref update. `commits` are the ids of the commits the update
/// introduced, newest first, `total` how many there were, and `render` renders the commit
/// at an index (only the listed ones and the head are rendered). `html_url` is the
/// repository's page.
pub struct PushEnvelope<'a> {
    pub update: PushedRef<'a>,
    pub html_url: &'a str,
    pub commits: &'a [&'a str],
    pub render: &'a dyn Fn(usize) -> Value,
    pub total: usize,
    /// The pushing account's name; `None` for deploy keys and mirrors.
    pub pusher: Option<&'a str>,
    pub repository: Value,
    pub sender: Option<Value>,
    pub organization: Option<Value>,
}

impl PushEnvelope<'_> {
    pub fn into_json(self) -> Value {
        let html_url = self.html_url;
        let before = self.update.before.unwrap_or(NULL_SHA);
        let after = self.update.after.unwrap_or(NULL_SHA);
        let compare = if self.update.created {
            format!("{html_url}/commit/{after}")
        } else if self.update.deleted {
            html_url.to_owned()
        } else {
            format!("{html_url}/compare/{}...{}", short(before), short(after))
        };
        // GitHub lists commits oldest first; `head_commit` is the new tip.
        let listed = self.commits.len().min(MAX_PUSH_COMMITS);
        let shown: Vec<Value> = (0..listed).rev().map(self.render).collect();
        let head_commit = self.commits.first().filter(|id| Some(**id) == self.update.after).map(|_| (self.render)(0));
        let pusher = json!({ "name": self.pusher.unwrap_or("deploy-key"), "email": null });
        let payload = json!({
            "ref": self.update.name,
            "before": before,
            "after": after,
            "created": self.update.created,
            "deleted": self.update.deleted,
            "base_ref": null,
            "compare": compare,
            "commits": shown,
            "total_commits": self.total,
            "head_commit": head_commit,
            "pusher": pusher,
            "repository": self.repository,
            "sender": self.sender,
        });
        with_organization(payload, self.organization)
    }
}

impl std::fmt::Debug for PushEnvelope<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PushEnvelope").field("update", &self.update).field("total", &self.total).finish_non_exhaustive()
    }
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

#[cfg(test)]
#[path = "payloads_tests.rs"]
mod tests;
