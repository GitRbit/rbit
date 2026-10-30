//! `pull_request` and `pull_request_review` webhook payloads.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! [`crate::WebhookService::handle_event`] calls [`payload`] for
//! `DomainEvent::Issue { kind: IssueKind::Pull, .. }` (including the label, milestone and
//! assignee actions I's services publish for pull requests) and every
//! `DomainEvent::PullRequestReview`.
//!
//! Shapes follow GitHub: `{action, number, pull_request, repository, sender}` plus
//! `changes` (edited), `before`/`after` (synchronize), `requested_reviewer`,
//! `label`, `assignee` and `milestone` where they apply; reviews carry `review`.

use rbit_core::{IssueKind, issue::IssueAction, issue::ReviewAction, webhook::WebhookEvent};
use serde_json::{Value, json};

use crate::{
    DomainEvent, ServiceContext, ServiceError,
    webhooks::{
        HookPayload,
        payloads::{account, repository, with_organization},
    },
};

/// The `action` of a `pull_request` delivery, or `None` for actions with no delivery.
pub fn pull_request_action(action: &IssueAction) -> Option<&'static str> {
    Some(match action {
        IssueAction::Opened => "opened",
        IssueAction::Edited { .. } => "edited",
        IssueAction::Closed { .. } => "closed",
        IssueAction::Reopened => "reopened",
        IssueAction::Locked => "locked",
        IssueAction::Unlocked => "unlocked",
        IssueAction::Labeled { .. } => "labeled",
        IssueAction::Unlabeled { .. } => "unlabeled",
        IssueAction::Assigned { .. } => "assigned",
        IssueAction::Unassigned { .. } => "unassigned",
        IssueAction::Milestoned { .. } => "milestoned",
        IssueAction::Demilestoned { .. } => "demilestoned",
        IssueAction::Synchronized { .. } => "synchronize",
        IssueAction::ReviewRequested { .. } => "review_requested",
        IssueAction::ReviewRequestRemoved { .. } => "review_request_removed",
        IssueAction::ConvertedToDraft => "converted_to_draft",
        IssueAction::ReadyForReview => "ready_for_review",
        IssueAction::Deleted { .. } => return None,
    })
}

/// The payload for `event`, or `None` when the event produces no delivery: no active hook
/// of the repository or its organisation subscribes to it, or the pull request or
/// repository is already gone.
///
/// Checking for subscribers first matters: the `pull_request` object diffs the pull
/// request (or reads its cached statistics), which would otherwise run for every pull
/// request event of every repository only to be thrown away.
pub async fn payload(ctx: &ServiceContext, event: &DomainEvent, base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    let (repo, hook_event) = match event {
        DomainEvent::Issue { repo, kind: IssueKind::Pull, .. } => (*repo, WebhookEvent::PullRequest),
        DomainEvent::PullRequestReview { repo, .. } => (*repo, WebhookEvent::PullRequestReview),
        _ => return Ok(None),
    };
    let Some(owned) = ctx.database.admin().find_repository(repo).await? else { return Ok(None) };
    let orgs = if owned.owner.is_organization() { vec![owned.owner.id] } else { Vec::new() };
    if ctx.database.webhooks().subscribed(Some(repo), &orgs, hook_event).await?.is_empty() {
        return Ok(None);
    }
    build(ctx, event, base_url).await
}

/// The payload for `event` whether or not any hook subscribes to it (see [`payload`]).
/// rbit Actions stores it as a pull request run's event (Actions: triggers (T)).
pub async fn build(ctx: &ServiceContext, event: &DomainEvent, base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    match event {
        DomainEvent::Issue { repo, issue, kind: IssueKind::Pull, actor, action, .. } => {
            let Some(name) = pull_request_action(action) else { return Ok(None) };
            let Some(loaded) = load(ctx, *repo, *issue, *actor).await? else { return Ok(None) };
            let mut body = base_body(ctx, &loaded, name, base_url).await?;
            match action {
                IssueAction::Edited { previous_title, previous_body, previous_base } => {
                    let mut changes = serde_json::Map::new();
                    if let Some(from) = previous_title {
                        changes.insert("title".to_owned(), json!({ "from": from }));
                    }
                    if let Some(from) = previous_body {
                        changes.insert("body".to_owned(), json!({ "from": from }));
                    }
                    if let Some(from) = previous_base {
                        changes.insert("base".to_owned(), json!({ "ref": { "from": from } }));
                    }
                    body["changes"] = Value::Object(changes);
                }
                IssueAction::Synchronized { before, after, .. } => {
                    body["before"] = json!(before);
                    body["after"] = json!(after);
                }
                IssueAction::ReviewRequested { reviewer } | IssueAction::ReviewRequestRemoved { reviewer } => {
                    body["requested_reviewer"] = account_json(ctx, *reviewer, base_url).await?;
                }
                IssueAction::Assigned { assignee } | IssueAction::Unassigned { assignee } => {
                    body["assignee"] = account_json(ctx, *assignee, base_url).await?;
                }
                IssueAction::Labeled { label } | IssueAction::Unlabeled { label } => {
                    body["label"] = crate::issues::json::label(label, &loaded.repo, base_url);
                }
                IssueAction::Milestoned { milestone } => {
                    body["milestone"] = json!({ "id": milestone.0 });
                }
                IssueAction::Demilestoned { milestone, title } => {
                    body["milestone"] = json!({ "id": milestone.0, "title": title });
                }
                _ => {}
            }
            Ok(Some(HookPayload {
                repo: loaded.repo.clone(),
                event: WebhookEvent::PullRequest,
                body: with_organization(body, &loaded.repo, base_url),
            }))
        }
        DomainEvent::PullRequestReview { repo, pull, review, actor, action, .. } => {
            let Some(loaded) = load(ctx, *repo, *pull, Some(*actor)).await? else { return Ok(None) };
            let Some(stored) = ctx.database.reviews().find(*pull, *review).await? else { return Ok(None) };
            let name = match action {
                ReviewAction::Submitted => "submitted",
                ReviewAction::Edited { .. } => "edited",
                ReviewAction::Dismissed => "dismissed",
            };
            let mut body = base_body(ctx, &loaded, name, base_url).await?;
            let reviewer = match stored.reviewer {
                Some(id) => ctx.database.accounts().find_accounts(&[id]).await?.pop(),
                None => None,
            };
            body["review"] = super::json::review(&stored, reviewer.as_ref(), &loaded.repo, loaded.issue.number.get(), base_url);
            if let ReviewAction::Edited { previous_body } = action {
                body["changes"] = json!({ "body": { "from": previous_body } });
            }
            Ok(Some(HookPayload {
                repo: loaded.repo.clone(),
                event: WebhookEvent::PullRequestReview,
                body: with_organization(body, &loaded.repo, base_url),
            }))
        }
        _ => Ok(None),
    }
}

struct Loaded {
    repo: rbit_core::OwnedRepository,
    issue: rbit_core::Issue,
    pull: rbit_core::pull::PullRequest,
    sender: Option<rbit_core::Account>,
}

async fn load(
    ctx: &ServiceContext,
    repo: rbit_core::RepoId,
    issue: rbit_core::IssueId,
    actor: Option<rbit_core::AccountId>,
) -> Result<Option<Loaded>, ServiceError> {
    let database = &ctx.database;
    let Some(repo) = database.admin().find_repository(repo).await? else { return Ok(None) };
    let Some(thread) = database.issues().find(issue).await? else { return Ok(None) };
    let Some(pull) = database.pulls().find(issue).await? else { return Ok(None) };
    let sender = match actor {
        Some(id) => database.accounts().find_accounts(&[id]).await?.pop(),
        None => None,
    };
    Ok(Some(Loaded { repo, issue: thread, pull, sender }))
}

async fn base_body(ctx: &ServiceContext, loaded: &Loaded, action: &str, base_url: &str) -> Result<Value, ServiceError> {
    let fork = ctx.database.forks().forks_among(&[loaded.repo.repo.id]).await?.contains(&loaded.repo.repo.id);
    Ok(json!({
        "action": action,
        "number": loaded.issue.number.get(),
        "pull_request": super::json::load(ctx, &loaded.issue, &loaded.pull, base_url).await?,
        "repository": repository(&loaded.repo, base_url, fork),
        "sender": loaded.sender.as_ref().map_or(Value::Null, |sender| account(sender, base_url)),
    }))
}

async fn account_json(ctx: &ServiceContext, id: rbit_core::AccountId, base_url: &str) -> Result<Value, ServiceError> {
    Ok(ctx.database.accounts().find_accounts(&[id]).await?.pop().map_or(Value::Null, |a| account(&a, base_url)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_follow_github() {
        assert_eq!(
            pull_request_action(&IssueAction::Synchronized {
                before: String::new(),
                after: String::new(),
                source: rbit_core::PushSource::System,
            }),
            Some("synchronize")
        );
        assert_eq!(pull_request_action(&IssueAction::Closed { reason: rbit_core::StateReason::Merged }), Some("closed"));
        assert_eq!(pull_request_action(&IssueAction::ReadyForReview), Some("ready_for_review"));
    }
}
