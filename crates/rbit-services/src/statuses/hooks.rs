//! The `status` webhook payload and the JSON shape of a status.
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file.
//!
//! [`crate::WebhookService::handle_event`] calls [`payload`] for every
//! `DomainEvent::CommitStatus`. The shape is GitHub's: the status fields at the top level
//! (`id`, `sha`, `name`, `state`, `context`, `description`, `target_url`, `created_at`,
//! `updated_at`), the `commit`, the `branches` whose tip it is, `repository`, `sender` and,
//! for organisation repositories, `organization`. There is no `action`.

use rbit_core::{Account, protection::is_protected, repository::OwnedRepository, status::CommitStatus, webhook::WebhookEvent};
use rbit_git::{CommitSummary, RefKind};
use serde_json::{Value, json};

use crate::{
    DomainEvent, RepoContext, ServiceContext, ServiceError,
    webhooks::{
        HookPayload,
        payloads::{account, repository, timestamp, with_organization},
    },
};

/// Branches listed in a payload (GitHub lists up to 10).
const MAX_BRANCHES: usize = 10;

/// A status as the REST API returns it: GitHub's fields, `creator` as an account (or
/// `null` once deleted).
pub fn status(status: &CommitStatus, creator: Option<&Account>, repo: &OwnedRepository, base_url: &str) -> Value {
    json!({
        "url": format!("{base_url}/api/v1/repos/{}/statuses/{}", repo.full_name(), status.sha),
        "id": status.id.0,
        "state": status.state.as_str(),
        "description": status.description,
        "target_url": status.target_url,
        "context": status.context.as_str(),
        "created_at": timestamp(status.created_at),
        "updated_at": timestamp(status.updated_at),
        "creator": creator.map(|who| account(who, base_url)),
    })
}

/// The `commit` object of a payload.
fn commit(summary: &CommitSummary, repo: &OwnedRepository, base_url: &str) -> Value {
    let full_name = repo.full_name();
    let message = match &summary.body {
        Some(body) if !body.is_empty() => format!("{}\n\n{body}", summary.summary),
        _ => summary.summary.clone(),
    };
    let person = json!({ "name": summary.author_name, "email": summary.author_email, "date": timestamp(summary.authored_at) });
    let parents: Vec<Value> = summary
        .parent_ids
        .iter()
        .map(|id| {
            json!({
                "sha": id,
                "url": format!("{base_url}/api/v1/repos/{full_name}/commits/{id}"),
                "html_url": format!("{base_url}/{full_name}/commit/{id}"),
            })
        })
        .collect();
    json!({
        "sha": summary.id,
        "commit": { "author": person, "committer": person, "message": message },
        "url": format!("{base_url}/api/v1/repos/{full_name}/commits/{}", summary.id),
        "html_url": format!("{base_url}/{full_name}/commit/{}", summary.id),
        "parents": parents,
    })
}

/// The payload for `event`, or `None` when it is not a status event or the status or its
/// repository is already gone.
pub async fn payload(ctx: &ServiceContext, event: &DomainEvent, base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    let DomainEvent::CommitStatus { repo, status: id, actor, .. } = event else { return Ok(None) };
    let Some(owned) = ctx.database.admin().find_repository(*repo).await? else { return Ok(None) };
    let Some(stored) = ctx.database.commit_statuses().find(*repo, *id).await? else { return Ok(None) };
    let system = RepoContext::for_system(owned.clone(), &ctx.repositories);
    let summary = system.reader.history(stored.sha.clone(), 0, 1).await.ok().and_then(|mut commits| commits.pop());
    let rules: Vec<_> = ctx.database.branch_protection().list(*repo).await?.into_iter().map(|rule| rule.rule).collect();
    let full_name = owned.full_name();
    let branches: Vec<Value> = system
        .reader
        .refs()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.kind == RefKind::Branch && r.target == stored.sha)
        .take(MAX_BRANCHES)
        .map(|branch| {
            json!({
                "name": branch.name,
                "commit": { "sha": branch.target, "url": format!("{base_url}/api/v1/repos/{full_name}/commits/{}", branch.target) },
                "protected": is_protected(&rules, &branch.name),
            })
        })
        .collect();
    let fork = ctx.database.forks().forks_among(&[*repo]).await?.contains(repo);
    let sender = ctx.database.accounts().find_accounts(&[*actor]).await?.pop();
    let body = json!({
        "id": stored.id.0,
        "sha": stored.sha,
        "name": full_name,
        "target_url": stored.target_url,
        "context": stored.context.as_str(),
        "description": stored.description,
        "state": stored.state.as_str(),
        "commit": summary.as_ref().map(|summary| commit(summary, &owned, base_url)),
        "branches": branches,
        "created_at": timestamp(stored.created_at),
        "updated_at": timestamp(stored.updated_at),
        "repository": repository(&owned, base_url, fork),
        "sender": sender.as_ref().map(|who| account(who, base_url)),
    });
    Ok(Some(HookPayload { body: with_organization(body, &owned, base_url), repo: owned, event: WebhookEvent::Status }))
}
