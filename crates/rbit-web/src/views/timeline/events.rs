//! Timeline events as sentences: a localized [`Event`] whose slots are filled with
//! fragments (the actor, a label, a commit, a branch…), so a translation may reorder them.
use rbit_core::{Account, IssueKind, StateReason, TimelineEvent, issue::MergeStrategy};
use rbit_localization::{CloseReason, TimelineEventMessage as Event, TimelineNoteMessage as Note};
use rbit_services::timeline::{EventSubjects, IssueContext};

use super::{ActorView, Fragment, TimelineOptions};
use crate::urls::{RepoUrls, issue_url};

/// An event's icon, its sentence (the actor first), whether that sentence is the English
/// fallback, and an optional muted detail line (a commit summary).
pub(super) struct Sentence {
    pub icon: &'static str,
    pub parts: Vec<Fragment>,
    pub fallback: bool,
    pub detail: Option<String>,
}

/// The sentence of `event` by `actor` in `options.locale`.
pub(super) fn sentence(
    actor: Option<&Account>,
    event: &TimelineEvent,
    subjects: &EventSubjects,
    urls: &RepoUrls,
    ctx: &IssueContext,
    options: TimelineOptions<'_>,
) -> Sentence {
    let actor_view = actor.map_or_else(ActorView::ghost, |account| ActorView::new(Some(account)));
    let mut slots = vec![Fragment { url: actor_view.url, ..Fragment::new("actor", actor_view.name) }];
    let (icon, message, detail) = message(actor, event, subjects, urls, ctx, options, &mut slots);
    let rich = message.rich(options.locale);
    let parts = rich
        .parts()
        .iter()
        .filter_map(|part| match part.slot() {
            Some(index) => slots.get(index).filter(|slot| !slot.kind.is_empty()).cloned(),
            None => Some(Fragment::text(part.text())),
        })
        .collect();
    Sentence { icon, parts, fallback: rich.fallback(), detail }
}

/// The icon, the message and the detail line of `event`; pushes the fragments of slots 1
/// and up onto `slots` (an empty kind marks a slot the message does not use).
#[expect(clippy::too_many_lines, reason = "one arm per event kind, kept together so none is missed")]
fn message(
    actor: Option<&Account>,
    event: &TimelineEvent,
    subjects: &EventSubjects,
    urls: &RepoUrls,
    ctx: &IssueContext,
    options: TimelineOptions<'_>,
    slots: &mut Vec<Fragment>,
) -> (&'static str, Event, Option<String>) {
    use Fragment as F;
    let unused = || F::new("", "");
    let repo = &ctx.repo.repo;
    let (owner, name) = (repo.owner.name.as_str(), repo.repo.name.as_str());
    let subject = || F::account(subjects.account.as_ref());
    let is_self = |account: rbit_core::AccountId| actor.is_some_and(|actor| actor.id == account);
    let note = |note: Note| note.localized(options.locale).to_string();
    let (icon, message) = match event {
        TimelineEvent::Closed { reason, commit, pull } => {
            let (icon, reason) = match reason {
                StateReason::Completed => ("circle-check", CloseReason::Completed),
                StateReason::NotPlanned => ("circle-slash", CloseReason::NotPlanned),
                StateReason::Merged => ("git-merge", CloseReason::Merged),
            };
            if let Some(commit) = commit {
                slots.push(F::commit(commit, urls));
            } else if let Some(pull) = pull {
                slots.push(F::link(pull.to_string(), issue_url(owner, name, IssueKind::Pull, *pull)));
            }
            (icon, if slots.len() > 1 { Event::ClosedIn { reason } } else { Event::Closed { reason } })
        }
        TimelineEvent::Reopened => ("circle-dot", Event::Reopened),
        TimelineEvent::Renamed { from, to } => {
            slots.extend([F::struck(from.clone()), F::strong(to.clone())]);
            ("pencil", Event::Renamed)
        }
        TimelineEvent::Locked => ("lock", Event::Locked),
        TimelineEvent::Unlocked => ("unlock", Event::Unlocked),
        TimelineEvent::Labeled { label } | TimelineEvent::Unlabeled { label } => {
            slots.push(F::label(label));
            ("tag", if matches!(event, TimelineEvent::Labeled { .. }) { Event::Labeled } else { Event::Unlabeled })
        }
        TimelineEvent::Milestoned { title, .. } | TimelineEvent::Demilestoned { title, .. } => {
            slots.push(F::strong(title.clone()));
            ("milestone", if matches!(event, TimelineEvent::Milestoned { .. }) { Event::Milestoned } else { Event::Demilestoned })
        }
        TimelineEvent::Assigned { assignee } if is_self(*assignee) => ("user-plus", Event::SelfAssigned),
        TimelineEvent::Assigned { .. } => {
            slots.push(subject());
            ("user-plus", Event::Assigned)
        }
        TimelineEvent::Unassigned { assignee } if is_self(*assignee) => ("user", Event::SelfUnassigned),
        TimelineEvent::Unassigned { .. } => {
            slots.push(subject());
            ("user", Event::Unassigned)
        }
        TimelineEvent::CommitReferenced { commit, summary } => {
            slots.push(F::commit(commit, urls));
            return ("commit", Event::CommitReferenced, Some(summary.clone()).filter(|s| !s.is_empty()));
        }
        TimelineEvent::CrossReferenced { .. } => {
            // The service drops cross-references the viewer may not see; a missing source
            // only happens for views built by hand.
            let Some(source) = &subjects.source else { return ("link", Event::Mentioned, None) };
            let (owner, name) = source.full_name.split_once('/').unwrap_or((source.full_name.as_str(), ""));
            let reference = if source.full_name == repo.full_name() {
                source.number.to_string()
            } else {
                format!("{}{}", source.full_name, source.number)
            };
            slots.push(F::link(format!("{} {reference}", source.title), issue_url(owner, name, source.kind, source.number)));
            ("link", if source.kind == IssueKind::Pull { Event::MentionedInPull } else { Event::MentionedInIssue })
        }
        TimelineEvent::HeadPushed { before, after, forced: true, .. } => {
            slots.extend([options.head_branch.map_or_else(unused, F::code), F::commit(before, urls), F::commit(after, urls)]);
            ("git-pull-request", if options.head_branch.is_some() { Event::ForcePushedBranch } else { Event::ForcePushed })
        }
        TimelineEvent::HeadPushed { after, count, .. } => {
            let count = u64::from(*count);
            slots.extend([options.head_branch.map_or_else(unused, F::code), F::commit(after, urls)]);
            ("commit", if options.head_branch.is_some() { Event::PushedTo { count } } else { Event::Pushed { count } })
        }
        TimelineEvent::BaseChanged { from, to } => {
            slots.extend([F::code(from.clone()), F::code(to.clone())]);
            ("branch", Event::BaseChanged)
        }
        TimelineEvent::Merged { commit, strategy } => {
            let how = match strategy {
                Some(MergeStrategy::Merge) => Note::MergeCommit,
                Some(MergeStrategy::Squash) => Note::Squash,
                Some(MergeStrategy::Rebase) => Note::Rebase,
                None => Note::OutsideInstance,
            };
            slots.extend([F::commit(commit, urls), options.base_branch.map_or_else(unused, F::code), F::muted(note(how))]);
            ("git-merge", if options.base_branch.is_some() { Event::MergedInto } else { Event::Merged })
        }
        TimelineEvent::ReviewRequested { reviewer } if is_self(*reviewer) => ("eye", Event::ReviewRequestedSelf),
        TimelineEvent::ReviewRequested { .. } | TimelineEvent::ReviewRequestRemoved { .. } => {
            slots.push(subject());
            if matches!(event, TimelineEvent::ReviewRequested { .. }) {
                ("eye", Event::ReviewRequested)
            } else {
                ("eye-off", Event::ReviewRequestRemoved)
            }
        }
        TimelineEvent::ReviewDismissed { review, reviewer } => {
            let link = F::link(note(Note::Review), format!("#pullrequestreview-{}", review.0));
            slots.extend([if reviewer.is_some() { subject() } else { unused() }, link]);
            ("x", if reviewer.is_some() { Event::ReviewDismissedOf } else { Event::ReviewDismissed })
        }
        TimelineEvent::ReviewThreadResolved { comment } | TimelineEvent::ReviewThreadReopened { comment } => {
            slots.push(F::link(note(Note::ReviewConversation), format!("#r{}", comment.0)));
            let resolved = matches!(event, TimelineEvent::ReviewThreadResolved { .. });
            ("message-square", if resolved { Event::ThreadResolved } else { Event::ThreadReopened })
        }
        TimelineEvent::HeadRefDeleted { branch } => {
            slots.push(F::code(branch.clone()));
            ("trash", Event::HeadRefDeleted)
        }
        TimelineEvent::ConvertedToDraft => ("git-pull-request-draft", Event::ConvertedToDraft),
        TimelineEvent::ReadyForReview => ("eye", Event::ReadyForReview),
    };
    (icon, message, None)
}
