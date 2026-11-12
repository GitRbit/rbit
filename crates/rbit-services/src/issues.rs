//! Issues: creation with metadata, labels, milestones, assignees, commit references and
//! search. The comments, state changes and timeline shared with pull requests live in
//! [`crate::timeline`].
//!
//! Milestone 2 workstream I (issues) owns this file and `issues/**`.
//!
//! Hooks other workstreams call (signatures fixed by `docs/design/milestone-2b.md` §3):
//! [`IssueService::metadata`], [`hooks::payload`] and [`json`].
//!
//! Labels, milestones and assignees attach to the shared `issues` row, so they work on
//! pull requests too: every change records its timeline event and publishes
//! `DomainEvent::Issue` with the thread's own kind. Triage (labels, milestone, assignees)
//! needs `can_triage` from [`rbit_core::issue::issue_permissions`], recomputed for the actor on every call.
//! Creating and changing things spends the actor's [`ThrottleScope::Content`] budget.
//!
//! The background subscriber ([`IssueService::spawn_background`]) turns references into
//! timeline entries: `#N` and `owner/repo#N` in threads and comments become
//! `CrossReferenced` events on their targets, `#N` in pushed commits `CommitReferenced`,
//! and closing keywords in commits pushed to the default branch (or in the body of a
//! merged pull request) close the issues they name.

mod assignee_attachments;
mod creation;
mod deletion;
mod reader;
// Agent loop: red main.
mod red_main;
pub use reader::IssueReader;
pub mod hooks;
pub mod json;
mod label_attachments;
mod labels;
mod milestone_assignment;
mod milestones;
mod ports;
use ports::{ActivityView, NamesView, ParticipantView, participant_error};

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub use rbit_collaboration::issues::{DEFAULT_DASHBOARD_QUERY, dashboard_query};
use rbit_collaboration::issues::{DashboardMentions, IssueParticipants, ThreadActivity};
use rbit_core::{
    Account, AccountId, Issue, IssueId, IssueKind, IssueNumber, LabelId, RepoId, StateReason, Timestamp, User, issue::IssueAction,
    repository::OwnedRepository,
};
#[cfg(test)]
use rbit_storage::sqlite::issue_meta::LabelFields;
#[cfg(test)]
use rbit_storage::sqlite::issue_meta::MilestoneFields;
pub use rbit_storage::sqlite::{
    issue_meta::{Label, LabelUsage, Milestone, MilestoneProgress},
    issue_search::{DashboardRelation, DashboardScope, SearchFilter},
};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    DomainEvent, RepoContext, ServiceContext, ServiceError, ThrottleScope, ThrottleService, TimelineService, audit::AuditService,
    events::RefUpdate, references::ReferenceService,
};

pub use rbit_collaboration::issues::ISSUES_PER_PAGE;
/// Most users [`IssueService::assignable`] lists.
pub use rbit_collaboration::issues::MAX_ASSIGNABLE;
pub use rbit_collaboration::issues::{MAX_ASSIGNEES, MAX_LABELS_PER_ISSUE};
pub use rbit_collaboration::milestones::{MAX_MILESTONE_DESCRIPTION_BYTES, MAX_MILESTONE_TITLE_CHARS};

/// How long shutdown waits for the subscriber.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

pub use rbit_collaboration::labels::{DEFAULT_LABELS, MAX_LABEL_DESCRIPTION_CHARS, MAX_LABEL_NAME_CHARS, MAX_LABELS_PER_REPO};

pub use rbit_collaboration::{
    issues::{IssueMetadata, IssueSearchResults, MetadataNeeds, NewIssueInput},
    labels::LabelInput,
    milestones::{MilestoneInput, MilestoneRef},
};

// ------------------------------------------------------------------ validation

/// `#rrggbb` (lower case) from `#RRGGBB` or `rrggbb`.
pub fn normalise_color(color: &str) -> Result<String, ServiceError> {
    rbit_collaboration::labels::normalise_color(color).map_err(ServiceError::Invalid)
}

#[cfg(test)]
fn label_fields(input: &LabelInput) -> Result<LabelFields, ServiceError> {
    rbit_collaboration::labels::label_fields(input).map_err(ServiceError::Invalid)
}

pub use rbit_collaboration::milestones::is_calendar_day;

#[cfg(test)]
fn milestone_fields(input: &MilestoneInput) -> Result<MilestoneFields, ServiceError> {
    rbit_collaboration::milestones::milestone_fields(input).map_err(ServiceError::Invalid)
}

/// Participant decisions ([`IssueParticipants`]) for the view's repository.
fn participants<'a>(view: &'a ParticipantView<'a>) -> IssueParticipants<'a, ParticipantView<'a>> {
    IssueParticipants::new(view, view.repo.repo.id)
}

// ------------------------------------------------------------------ the service

#[derive(Debug)]
struct Background {
    started: AtomicBool,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

#[derive(Debug, Clone)]
pub struct IssueService {
    ctx: ServiceContext,
    timeline: TimelineService,
    references: ReferenceService,
    throttle: ThrottleService,
    audit: AuditService,
    background: Arc<Background>,
}

impl IssueService {
    pub(crate) fn new(ctx: ServiceContext, timeline: TimelineService, references: ReferenceService, throttle: ThrottleService) -> Self {
        let audit = AuditService::from_store(ctx.database.audit());
        let background =
            Arc::new(Background { started: AtomicBool::new(false), shutdown: CancellationToken::new(), tasks: TaskTracker::new() });
        Self { ctx, timeline, references, throttle, audit, background }
    }

    fn spend(&self, actor: &User) -> Result<(), ServiceError> {
        self.throttle.check(ThrottleScope::Content, &actor.id().0.to_string())
    }

    fn publish(&self, issue: &Issue, actor: Option<AccountId>, action: IssueAction) {
        self.ctx.events.publish(DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            actor,
            action,
        });
    }

    /// The users among `names` (case-insensitively); an unknown name is invalid.
    pub async fn users_named(&self, names: &[String]) -> Result<Vec<Account>, ServiceError> {
        rbit_collaboration::issues::users_named(&NamesView { service: self }, names).await.map_err(participant_error)
    }

    /// The number of the thread of `repo` that comment `id` belongs to.
    pub async fn comment_thread(&self, repo: &RepoContext, id: rbit_core::CommentId) -> Result<IssueNumber, ServiceError> {
        let view = ParticipantView { service: self, repo: &repo.repo };
        participants(&view).comment_thread(id).await.map_err(participant_error)
    }

    // ------------------------------------------------------------------ triage

    /// The labels of `repo` with these ids; an id that is not one of them is invalid.
    async fn repo_labels(&self, repo: &OwnedRepository, ids: &[LabelId]) -> Result<Vec<Label>, ServiceError> {
        let view = ParticipantView { service: self, repo };
        participants(&view).labels(ids).await.map_err(participant_error)
    }

    /// The users with these ids, each able to push to `repo`; anyone else is invalid.
    async fn validated_assignees(&self, repo: &OwnedRepository, ids: &[AccountId]) -> Result<Vec<User>, ServiceError> {
        let view = ParticipantView { service: self, repo };
        participants(&view).assignees(ids).await.map_err(participant_error)
    }

    /// People who may be assigned in `repo`: names starting with `prefix`, at most 100.
    pub async fn assignable(&self, repo: &RepoContext, prefix: &str) -> Result<Vec<Account>, ServiceError> {
        let view = ParticipantView { service: self, repo: &repo.repo };
        participants(&view).assignable(repo.access.can_read(), prefix).await.map_err(participant_error)
    }

    // ------------------------------------------------------------------ references in the background

    /// Starts this service's background work (cross-references, commit references and
    /// closing keywords). Called once from [`crate::Services::spawn_background`].
    pub fn spawn_background(&self) {
        if self.background.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let events = self.ctx.events.subscribe();
        let service = self.clone();
        self.background.tasks.spawn(async move { service.listen(events).await });
    }

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    pub async fn shutdown(&self) {
        self.background.shutdown.cancel();
        self.background.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.background.tasks.wait()).await.is_err() {
            tracing::warn!("issue references did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    async fn listen(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        loop {
            let event = tokio::select! {
                () = self.background.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(event) => {
                    if let Err(error) = self.handle_event(&event).await {
                        tracing::warn!(%error, ?event, "cannot record issue references");
                    }
                }
                Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "issue reference subscriber fell behind"),
                Err(RecvError::Closed) => break,
            }
        }
    }

    /// Dashboard mention recording ([`DashboardMentions`]) over the database.
    fn mentions(&self) -> DashboardMentions<'_, rbit_storage::Database, fn(&str) -> Vec<String>> {
        let scan: fn(&str) -> Vec<String> = |text| rbit_collaboration::references::scan_markdown(text).mentions;
        DashboardMentions::new(&self.ctx.database, scan)
    }

    /// One-time Markdown-aware upgrade of existing issue, comment and submitted review bodies.
    pub(crate) async fn index_existing_mentions(&self) -> Result<(), ServiceError> {
        Ok(self.mentions().index_existing().await?)
    }

    /// Reacts to one event: what the background subscriber does, callable directly (tests).
    /// Only routing happens here; [`ThreadActivity`] and the reference applications decide.
    pub async fn handle_event(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        use rbit_core::issue::{CommentAction, ReviewAction};
        let view = ActivityView { service: self };
        let activity = ThreadActivity::new(&view);
        match event {
            DomainEvent::Issue { issue, action: IssueAction::Edited { previous_body: Some(body), .. }, .. }
            | DomainEvent::IssueComment { issue, action: CommentAction::Edited { previous_body: body }, .. } => {
                activity.earlier_text(*issue, body).await?;
            }
            DomainEvent::IssueComment { issue, action: CommentAction::Deleted { snapshot }, .. } => {
                activity.earlier_text(*issue, snapshot.body.as_str()).await?;
            }
            _ => {}
        }
        match event {
            DomainEvent::PullRequestReview { pull, review, action, .. } => {
                let previous = match action {
                    ReviewAction::Edited { previous_body } => Some(previous_body.as_str()),
                    _ => None,
                };
                activity.review_changed(*pull, *review, previous).await
            }
            DomainEvent::RepositoryPushed { repo, source, updates } => self.scan_push(*repo, source.pusher(), updates).await,
            DomainEvent::Issue { issue, actor, action: IssueAction::Opened | IssueAction::Edited { .. }, .. } => {
                activity.thread_written(*issue, *actor).await
            }
            DomainEvent::IssueComment { issue, comment, actor, action: CommentAction::Created | CommentAction::Edited { .. }, .. } => {
                activity.comment_written(*issue, *comment, *actor).await
            }
            DomainEvent::Issue {
                kind: IssueKind::Pull,
                issue,
                number,
                actor,
                action: IssueAction::Closed { reason: StateReason::Merged },
                ..
            } => self.close_by_merge(*issue, *number, *actor).await,
            DomainEvent::ActionsRun { repo, run, action: crate::actions::hooks::RunAction::Completed } => self.red_main(*repo, *run).await,
            _ => Ok(()),
        }
    }

    /// Commits pushed to `repo`: `#N` references recorded once per (issue, commit); closing
    /// keywords close issues when the branch is the default branch. Mirrors are skipped.
    async fn scan_push(&self, repo: RepoId, pusher: Option<AccountId>, updates: &[RefUpdate]) -> Result<(), ServiceError> {
        let database = &self.ctx.database;
        if updates.is_empty() || database.mirrors().is_mirror(repo).await? {
            return Ok(());
        }
        let Some(owned) = database.admin().find_repository(repo).await? else { return Ok(()) };
        let reader = self.ctx.repositories.reader(owned.repo.owner_id, owned.repo.id);
        let snapshot = reader.ref_snapshot().await.ok();
        let limit = self.ctx.config.issues.scan_commit_limit;
        for update in updates {
            let (Some(branch), Some(after)) = (update.branch(), &update.after) else { continue };
            let tips = snapshot.as_ref().map(rbit_git::RefSnapshot::ids);
            let excluding = rbit_collaboration::references::push_exclusions(update.before.as_deref(), tips, after);
            let commits = match reader.new_commits(after.clone(), excluding, limit).await {
                Ok(commits) => commits,
                Err(error) => {
                    tracing::warn!(%error, repo = %owned.full_name(), branch, "cannot list pushed commits for references");
                    continue;
                }
            };
            let default_branch = branch == owned.repo.default_branch;
            // Preserve the original oldest-first Git iterator without a DTO Vec or OID copies.
            let effects = self.references.effects();
            let commits = commits.iter().rev().map(|commit| rbit_collaboration::references::CommitReference {
                id: &commit.id,
                summary: &commit.summary,
                body: commit.body.as_deref(),
            });
            rbit_collaboration::references::CommitReferences::new(&effects)
                .record(repo, pusher, default_branch, commits, Timestamp::now)
                .await?;
        }
        Ok(())
    }

    /// A pull request merged into the default branch closes the issues its body names
    /// with closing keywords (`pulls::lifecycle::merged_into_default_branch`); merges into
    /// other branches close nothing, as on GitHub.
    async fn close_by_merge(&self, pull: IssueId, number: IssueNumber, actor: Option<AccountId>) -> Result<(), ServiceError> {
        let effects = self.references.effects();
        rbit_collaboration::references::MergeIssueClosures::new(&effects)
            .close(pull, number, actor, rbit_collaboration::references::scan_markdown)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_normalised_strictly() {
        assert_eq!(normalise_color("#D73A4A").unwrap(), "#d73a4a");
        assert_eq!(normalise_color(" 0075ca ").unwrap(), "#0075ca");
        for bad in ["", "#fff", "red", "#1234567", "#12345g", "##123456"] {
            assert!(normalise_color(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn calendar_days() {
        for good in ["2026-10-16", "2024-02-29", "2000-02-29", "0001-01-01"] {
            assert!(is_calendar_day(good), "{good}");
        }
        for bad in [
            "2026-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-04-31",
            "2026-1-1",
            "26-10-16",
            "2026/10/16",
            "0000-01-01",
            "2026-10-1x",
        ] {
            assert!(!is_calendar_day(bad), "{bad}");
        }
    }

    #[test]
    fn label_and_milestone_fields_are_validated() {
        let ok = label_fields(&LabelInput { name: " bug ".into(), color: "D73A4A".into(), description: " Broken ".into() }).unwrap();
        assert_eq!((ok.name.as_str(), ok.color.as_str(), ok.description.as_str()), ("bug", "#d73a4a", "Broken"));
        assert!(label_fields(&LabelInput { name: " ".into(), color: "#000000".into(), description: String::new() }).is_err());
        assert!(label_fields(&LabelInput { name: "x".repeat(51), color: "#000000".into(), description: String::new() }).is_err());
        assert!(label_fields(&LabelInput { name: "x".into(), color: "#000000".into(), description: "d".repeat(101) }).is_err());
        assert!(label_fields(&LabelInput { name: "a\u{7}".into(), color: "#000000".into(), description: String::new() }).is_err());

        let fields =
            milestone_fields(&MilestoneInput { title: " v1 ".into(), description: "a\r\nb".into(), due_on: String::new() }).unwrap();
        assert_eq!((fields.title.as_str(), fields.description.as_str(), fields.due_on), ("v1", "a\nb", None));
        assert!(milestone_fields(&MilestoneInput { title: "v1".into(), due_on: "tomorrow".into(), ..MilestoneInput::default() }).is_err());
        assert!(milestone_fields(&MilestoneInput { title: "x".repeat(101), ..MilestoneInput::default() }).is_err());
        let long = "x".repeat(MAX_MILESTONE_DESCRIPTION_BYTES + 1);
        assert!(milestone_fields(&MilestoneInput { title: "v".into(), description: long, ..MilestoneInput::default() }).is_err());
    }

    #[test]
    fn default_labels_are_valid() {
        for (name, color, description) in DEFAULT_LABELS {
            let input = LabelInput { name: name.into(), color: color.into(), description: description.into() };
            assert_eq!(label_fields(&input).unwrap().color, color);
        }
    }
}
