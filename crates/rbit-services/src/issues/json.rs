//! GitHub-shaped JSON for issues, labels and milestones, shared by webhook payloads and
//! the REST API. P embeds these objects in `pull_request` payloads.
//!
//! Milestone 2 workstream I (issues) owns this file. The stubs build minimal objects
//! from core fields; I completes them (`user`, `labels`, `assignees`, `milestone`, …).

use rbit_core::{Issue, IssueKind, StateReason, issue::LabelSnapshot, repository::OwnedRepository};
use serde_json::{Value, json};

use crate::{issues::MilestoneRef, webhooks::payloads::timestamp};

/// An issue (or the issue side of a pull request), as payloads and the API embed it.
pub fn issue(issue: &Issue, repo: &OwnedRepository, base_url: &str) -> Value {
    let full_name = repo.full_name();
    let number = issue.number.get();
    let api = format!("{base_url}/api/v1/repos/{full_name}");
    let page = match issue.kind {
        IssueKind::Issue => "issues",
        IssueKind::Pull => "pull",
    };
    json!({
        "id": issue.id.0,
        "number": number,
        "title": issue.title.as_str(),
        "body": issue.body.as_str(),
        "state": issue.state.as_str(),
        "state_reason": issue.state_reason.map(StateReason::as_str),
        "locked": issue.locked,
        "comments": issue.comment_count,
        "created_at": timestamp(issue.created_at),
        "updated_at": timestamp(issue.updated_at),
        "closed_at": issue.closed_at.map(timestamp),
        "html_url": format!("{base_url}/{full_name}/{page}/{number}"),
        "url": format!("{api}/issues/{number}"),
        "repository_url": api.clone(),
        "comments_url": format!("{api}/issues/{number}/comments"),
    })
}

/// A label (`color` without the `#`, as GitHub returns it).
pub fn label(label: &LabelSnapshot, repo: &OwnedRepository, base_url: &str) -> Value {
    json!({
        "id": label.id.0,
        "name": label.name,
        "color": label.color.trim_start_matches('#'),
        "url": format!("{base_url}/api/v1/repos/{}/labels/{}", repo.full_name(), label.name),
    })
}

/// A milestone.
pub fn milestone(milestone: &MilestoneRef, repo: &OwnedRepository, base_url: &str) -> Value {
    json!({
        "id": milestone.id.0,
        "title": milestone.title,
        "url": format!("{base_url}/api/v1/repos/{}/milestones/{}", repo.full_name(), milestone.id.0),
    })
}
