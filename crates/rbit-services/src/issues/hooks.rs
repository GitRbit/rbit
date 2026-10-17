//! `issues` and `issue_comment` webhook payloads.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! [`crate::WebhookService::handle_event`] calls [`payload`] for
//! `DomainEvent::Issue { kind: IssueKind::Issue, .. }` and every
//! `DomainEvent::IssueComment` (comments on pull requests included: their `issue` object
//! carries a `pull_request` key). It queues the returned body for every hook subscribed
//! to [`HookPayload::event`] on the repository or its organisation.
//!
//! Shapes follow GitHub: `action`, `issue`, `comment` (comments), `changes` (edits),
//! `label`, `assignee` or `milestone` where the action names one, `repository`, `sender`,
//! and `organization` for organisation repositories.

use rbit_core::{
    Account, AccountId, Issue, IssueKind,
    issue::{CommentAction, IssueAction},
    repository::OwnedRepository,
    webhook::WebhookEvent,
};
use serde_json::{Value, json};

use super::{IssueMetadata, MilestoneRef, json as issue_json};
use crate::{
    DomainEvent, ServiceContext, ServiceError,
    webhooks::{
        HookPayload,
        payloads::{account, repository, with_organization},
    },
};

/// The payload for `event`, or `None` when the event produces no delivery (or the issue
/// or repository is already gone).
pub async fn payload(ctx: &ServiceContext, event: &DomainEvent, base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    match event {
        DomainEvent::Issue { repo, issue, kind: IssueKind::Issue, actor, action, .. } => {
            let Some(repo) = ctx.database.admin().find_repository(*repo).await? else { return Ok(None) };
            let (name, extra) = match action {
                IssueAction::Opened => ("opened", None),
                IssueAction::Edited { previous_title, previous_body, .. } => {
                    let mut changes = serde_json::Map::new();
                    if let Some(title) = previous_title {
                        changes.insert("title".into(), json!({ "from": title }));
                    }
                    if let Some(body) = previous_body {
                        changes.insert("body".into(), json!({ "from": body }));
                    }
                    ("edited", Some(("changes", Value::Object(changes))))
                }
                IssueAction::Closed { .. } => ("closed", None),
                IssueAction::Reopened => ("reopened", None),
                IssueAction::Locked => ("locked", None),
                IssueAction::Unlocked => ("unlocked", None),
                IssueAction::Deleted { .. } => ("deleted", None),
                IssueAction::Labeled { label } => ("labeled", Some(("label", issue_json::label(label, &repo, base_url)))),
                IssueAction::Unlabeled { label } => ("unlabeled", Some(("label", issue_json::label(label, &repo, base_url)))),
                IssueAction::Assigned { assignee } | IssueAction::Unassigned { assignee } => {
                    let name = if matches!(action, IssueAction::Assigned { .. }) { "assigned" } else { "unassigned" };
                    let assignee = find_account(ctx, *assignee).await?;
                    (name, Some(("assignee", assignee.map_or(Value::Null, |a| account(&a, base_url)))))
                }
                IssueAction::Milestoned { milestone } => {
                    let Some(found) = ctx.database.issue_meta().find_milestone(repo.repo.id, *milestone).await? else { return Ok(None) };
                    let reference = MilestoneRef { id: found.id, title: found.title };
                    ("milestoned", Some(("milestone", issue_json::milestone(&reference, &repo, base_url))))
                }
                IssueAction::Demilestoned { milestone, title } => {
                    let reference = MilestoneRef { id: *milestone, title: title.clone() };
                    ("demilestoned", Some(("milestone", issue_json::milestone(&reference, &repo, base_url))))
                }
                _ => return Ok(None),
            };
            let row = match action {
                IssueAction::Deleted { snapshot } => Some((**snapshot).clone()),
                _ => ctx.database.issues().find(*issue).await?,
            };
            let Some(row) = row else { return Ok(None) };
            let mut body = json!({ "action": name, "issue": issue_object(ctx, &row, &repo, base_url).await? });
            if let Some((key, value)) = extra {
                body[key] = value;
            }
            Ok(Some(finish(ctx, body, &repo, *actor, WebhookEvent::Issues, base_url).await?))
        }
        DomainEvent::IssueComment { repo, issue, comment, actor, action, .. } => {
            let Some(repo) = ctx.database.admin().find_repository(*repo).await? else { return Ok(None) };
            let Some(row) = ctx.database.issues().find(*issue).await? else { return Ok(None) };
            let (name, stored) = match action {
                CommentAction::Created => ("created", ctx.database.issues().find_comment(*issue, *comment).await?),
                CommentAction::Edited { .. } => ("edited", ctx.database.issues().find_comment(*issue, *comment).await?),
                CommentAction::Deleted { snapshot } => ("deleted", Some(snapshot.clone())),
            };
            let Some(stored) = stored else { return Ok(None) };
            let author = match stored.author {
                Some(id) => find_account(ctx, id).await?,
                None => None,
            };
            let mut body = json!({
                "action": name,
                "issue": issue_object(ctx, &row, &repo, base_url).await?,
                "comment": issue_json::comment(&stored, author.as_ref(), &row, &repo, base_url),
            });
            if let CommentAction::Edited { previous_body } = action {
                body["changes"] = json!({ "body": { "from": previous_body } });
            }
            Ok(Some(finish(ctx, body, &repo, Some(*actor), WebhookEvent::IssueComment, base_url).await?))
        }
        _ => Ok(None),
    }
}

/// The full `issue` object: author and metadata loaded.
async fn issue_object(ctx: &ServiceContext, issue: &Issue, repo: &OwnedRepository, base_url: &str) -> Result<Value, ServiceError> {
    let author = match issue.author {
        Some(id) => find_account(ctx, id).await?,
        None => None,
    };
    let metadata = load_metadata(ctx, issue).await?;
    Ok(issue_json::issue_with(issue, author.as_ref(), &metadata, repo, base_url))
}

async fn load_metadata(ctx: &ServiceContext, issue: &Issue) -> Result<IssueMetadata, ServiceError> {
    let store = ctx.database.issue_meta();
    let ids = [issue.id];
    let (labels, assignees, milestones) = tokio::try_join!(store.labels_of(&ids), store.assignees_of(&ids), store.milestones_of(&ids))?;
    Ok(IssueMetadata {
        labels: labels.into_iter().map(|(_, label)| label).collect(),
        assignees: assignees.into_iter().map(|(_, account)| account).collect(),
        milestone: milestones.into_iter().next().map(|m| MilestoneRef { id: m.id, title: m.title }),
    })
}

/// Adds `repository`, `sender` and `organization`.
async fn finish(
    ctx: &ServiceContext,
    mut body: Value,
    repo: &OwnedRepository,
    actor: Option<AccountId>,
    event: WebhookEvent,
    base_url: &str,
) -> Result<HookPayload, ServiceError> {
    let fork = ctx.database.forks().forks_among(&[repo.repo.id]).await?.contains(&repo.repo.id);
    body["repository"] = repository(repo, base_url, fork);
    let sender = match actor {
        Some(id) => find_account(ctx, id).await?,
        None => None,
    };
    body["sender"] = sender.map_or(Value::Null, |sender| account(&sender, base_url));
    Ok(HookPayload { repo: repo.clone(), event, body: with_organization(body, repo, base_url) })
}

async fn find_account(ctx: &ServiceContext, id: AccountId) -> Result<Option<Account>, ServiceError> {
    Ok(ctx.database.accounts().find_accounts(&[id]).await?.pop())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rbit_core::{Config, IssueNumber, StateReason, User, Visibility};
    use rbit_git::RepositoryStorage;
    use rbit_storage::{Database, KvStore, NewUser};

    use super::*;
    use crate::{
        CreateRepository, Services,
        issues::{LabelInput, MilestoneInput, NewIssueInput},
        timeline::StateChange,
    };

    const BASE: &str = "https://git.example.com";

    struct Fixture {
        _dir: tempfile::TempDir,
        ctx: ServiceContext,
        services: Services,
        alice: User,
    }

    impl Fixture {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
            let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
            let mut config = Config::default();
            config.storage.data_dir = dir.path().to_path_buf();
            let config = Arc::new(config);
            let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
            let ctx = ServiceContext {
                repositories: RepositoryStorage::new(config.storage.repositories_dir()),
                config,
                database: database.clone(),
                kv,
                events: services.events.clone(),
            };
            let new = NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "x".into(),
                is_admin: false,
            };
            let alice = database.accounts().create_user(new).await.unwrap();
            let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
            services.repositories.create(&alice, input).await.unwrap();
            Self { _dir: dir, ctx, services, alice }
        }

        async fn body(&self, event: &DomainEvent) -> Option<Value> {
            payload(&self.ctx, event, BASE).await.unwrap().map(|payload| payload.body)
        }
    }

    #[tokio::test]
    async fn issue_payloads_carry_github_fields() {
        let fixture = Fixture::new().await;
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "demo").await.unwrap();
        let label = LabelInput { name: "bug".into(), color: "#d73a4a".into(), description: String::new() };
        let bug = fixture.services.issues.create_label(&fixture.alice, &repo, &label).await.unwrap();
        let milestone = MilestoneInput { title: "v1".into(), ..MilestoneInput::default() };
        let v1 = fixture.services.issues.create_milestone(&fixture.alice, &repo, &milestone).await.unwrap();
        let input = NewIssueInput {
            title: "Crash".into(),
            body: "Boom".into(),
            labels: vec![bug.id],
            assignees: vec![fixture.alice.id()],
            milestone: Some(v1.id),
        };
        let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
        let event = |action| DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: IssueKind::Issue,
            actor: Some(fixture.alice.id()),
            action,
        };

        let opened = fixture.body(&event(IssueAction::Opened)).await.unwrap();
        assert_eq!(opened["action"], "opened");
        assert_eq!(opened["issue"]["number"], 1);
        assert_eq!(opened["issue"]["user"]["login"], "alice");
        assert_eq!(opened["issue"]["labels"][0]["name"], "bug");
        assert_eq!(opened["issue"]["assignees"][0]["login"], "alice");
        assert_eq!(opened["issue"]["milestone"]["title"], "v1");
        assert_eq!(opened["repository"]["full_name"], "alice/demo");
        assert_eq!(opened["sender"]["login"], "alice");
        assert!(opened.get("organization").is_none(), "only for organisation repositories");
        assert!(!opened.to_string().contains("alice@example.com"), "no email addresses");

        let edited = IssueAction::Edited { previous_title: Some("Old".into()), previous_body: None, previous_base: None };
        let edited = fixture.body(&event(edited)).await.unwrap();
        assert_eq!(edited["changes"], json!({ "title": { "from": "Old" } }));
        let labeled = fixture.body(&event(IssueAction::Labeled { label: bug.snapshot() })).await.unwrap();
        assert_eq!((labeled["action"].as_str(), labeled["label"]["color"].as_str()), (Some("labeled"), Some("d73a4a")));
        let assigned = fixture.body(&event(IssueAction::Assigned { assignee: fixture.alice.id() })).await.unwrap();
        assert_eq!(assigned["assignee"]["login"], "alice");
        let milestoned = fixture.body(&event(IssueAction::Milestoned { milestone: v1.id })).await.unwrap();
        assert_eq!(milestoned["milestone"]["title"], "v1");
        let demilestoned = fixture.body(&event(IssueAction::Demilestoned { milestone: v1.id, title: "v1".into() })).await.unwrap();
        assert_eq!(demilestoned["action"], "demilestoned");
        for (action, name) in [
            (IssueAction::Closed { reason: StateReason::Completed }, "closed"),
            (IssueAction::Reopened, "reopened"),
            (IssueAction::Locked, "locked"),
            (IssueAction::Unlocked, "unlocked"),
        ] {
            assert_eq!(fixture.body(&event(action)).await.unwrap()["action"], name);
        }
        assert_eq!(fixture.body(&event(IssueAction::ConvertedToDraft)).await, None, "pull request actions are not issue payloads");

        let ctx = fixture.services.timeline.open(Some(&fixture.alice), repo.clone(), issue.number).await.unwrap();
        fixture.services.timeline.set_state(&fixture.alice, &ctx, StateChange::Close(StateReason::NotPlanned)).await.unwrap();
        fixture.services.issues.delete(&fixture.alice, &ctx).await.unwrap();
        let snapshot = Box::new(ctx.issue.clone());
        let deleted = fixture.body(&event(IssueAction::Deleted { snapshot })).await.unwrap();
        assert_eq!((deleted["action"].as_str(), deleted["issue"]["title"].as_str()), (Some("deleted"), Some("Crash")));
        assert_eq!(fixture.body(&event(IssueAction::Opened)).await, None, "the issue is gone");
    }

    #[tokio::test]
    async fn comment_payloads_include_the_comment_and_pull_requests() {
        let fixture = Fixture::new().await;
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "demo").await.unwrap();
        let pull = fixture.services.timeline.create(&fixture.alice, &repo, IssueKind::Pull, "Fix", "").await.unwrap();
        let ctx = fixture.services.timeline.open(Some(&fixture.alice), repo, IssueNumber::new(1).unwrap()).await.unwrap();
        let comment = fixture.services.timeline.comment(&fixture.alice, &ctx, "Looks good").await.unwrap();
        let event = |action| DomainEvent::IssueComment {
            repo: pull.repo_id,
            issue: pull.id,
            number: pull.number,
            kind: IssueKind::Pull,
            comment: comment.id,
            actor: fixture.alice.id(),
            action,
        };
        let created = fixture.body(&event(CommentAction::Created)).await.unwrap();
        assert_eq!(created["action"], "created");
        assert_eq!(created["comment"]["body"], "Looks good");
        assert_eq!(created["comment"]["user"]["login"], "alice");
        assert_eq!(created["issue"]["pull_request"]["html_url"], "https://git.example.com/alice/demo/pull/1");
        let edited = fixture.body(&event(CommentAction::Edited { previous_body: "Looks bad".into() })).await.unwrap();
        assert_eq!(edited["changes"]["body"]["from"], "Looks bad");
        fixture.services.timeline.delete_comment(&fixture.alice, &ctx, comment.id).await.unwrap();
        assert_eq!(fixture.body(&event(CommentAction::Created)).await, None, "the comment is gone");
        let deleted = fixture.body(&event(CommentAction::Deleted { snapshot: comment.clone() })).await.unwrap();
        assert_eq!(deleted["comment"]["id"], comment.id.0);
    }
}
