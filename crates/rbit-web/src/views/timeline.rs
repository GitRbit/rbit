//! The conversation of an issue or pull request: the opening post, comments, every
//! timeline event as a sentence, the state badge and the comment form.
//!
//! The integrator owns this file and `templates/timeline/**` (T0 in
//! `docs/design/milestone-2b.md`, frozen for I and P). Issue pages (I) and pull request
//! pages (P) build a [`ThreadView`] with [`thread`] and render its parts:
//!
//! * `timeline/thread.html` (expects `thread: ThreadView`): opening post, items, "load
//!   more" and the comment form, in that order;
//! * `timeline/item.html` (expects `item: TimelineItemView`), for pages that interleave
//!   their own items (P's reviews) by [`TimelineItemView::cursor`];
//! * `timeline/state_badge.html` (`badge: StateBadge`), `timeline/title_form.html`
//!   (`title: TitleView`) and `timeline/comment_form.html` (`form: CommentFormView`).
//!
//! Edit forms appear in place with `?edit=comment-{id}`, `?edit=body` or `?edit=title`
//! (pass the query value as [`TimelineOptions::edit`]); htmx swaps the same fragment, so
//! nothing needs JavaScript. Anchors: `#issue-body` (opening post), `#issuecomment-{id}`,
//! `#event-{id}`, `#new-comment` (the form) and `#issue-title`. Deleted accounts appear
//! as **ghost**: no link, a neutral avatar.

use rbit_core::{
    Account, CommentId, EventId, Issue, IssueKind, IssueNumber, IssueState, StateReason, TimelineEvent, Timestamp, User,
    issue::{LabelSnapshot, MergeStrategy},
};
use rbit_services::{
    ServiceError,
    timeline::{EventSubjects, IssueContext, MAX_TIMELINE_PAGE, Timeline, TimelineCursor, TimelineEntry},
};

use crate::{
    AppState,
    render::references::{RefContext, render_issue_markdown},
    urls::{DocumentBase, RepoUrls, issue_url},
    views::components::Avatar,
};

/// The opening post's anchor.
pub const BODY_ANCHOR: &str = "issue-body";
/// The comment form's anchor (state and lock changes land here).
pub const NEW_COMMENT_ANCHOR: &str = "new-comment";
/// The title's anchor (title edits land here).
pub const TITLE_ANCHOR: &str = "issue-title";

/// `issuecomment-{id}`.
pub fn comment_anchor(id: CommentId) -> String {
    format!("issuecomment-{}", id.0)
}

/// `event-{id}`.
pub fn event_anchor(id: EventId) -> String {
    format!("event-{}", id.0)
}

/// Commits are shown by their first seven hex digits.
fn short_sha(commit: &str) -> String {
    commit.chars().take(7).collect()
}

// ------------------------------------------------------------------ URLs

/// The page and the form targets of one thread (`routes/timeline.rs`).
#[derive(Debug, Clone)]
pub struct ThreadUrls {
    /// `/{owner}/{repo}/issues/{n}` or `/{owner}/{repo}/pull/{n}`, by kind.
    pub page: String,
    /// `POST`: add a comment (optionally with `state`).
    pub comments: String,
    pub title: String,
    pub body: String,
    pub state: String,
    pub lock: String,
}

impl ThreadUrls {
    pub fn new(owner: &str, repo: &str, kind: IssueKind, number: IssueNumber) -> Self {
        let forms = format!("/{owner}/{repo}/issues/{}", number.get());
        Self {
            page: issue_url(owner, repo, kind, number),
            comments: format!("{forms}/comments"),
            title: format!("{forms}/title"),
            body: format!("{forms}/body"),
            state: format!("{forms}/state"),
            lock: format!("{forms}/lock"),
        }
    }

    pub fn for_context(ctx: &IssueContext) -> Self {
        let repo = &ctx.repo.repo;
        Self::new(repo.owner.name.as_str(), repo.repo.name.as_str(), ctx.issue.kind, ctx.issue.number)
    }

    /// `POST`: edit comment `id`.
    pub fn comment(&self, id: CommentId) -> String {
        format!("{}/{}", self.comments, id.0)
    }

    /// `POST`: delete comment `id`.
    pub fn delete_comment(&self, id: CommentId) -> String {
        format!("{}/{}/delete", self.comments, id.0)
    }

    /// The page with `?edit={what}`, scrolled to `anchor`.
    fn editing(&self, what: &str, anchor: &str) -> String {
        format!("{}?edit={what}#{anchor}", self.page)
    }

    /// The page scrolled to `anchor`.
    pub fn at(&self, anchor: &str) -> String {
        format!("{}#{anchor}", self.page)
    }
}

// ------------------------------------------------------------------ pieces

/// Who wrote or did something. Deleted accounts are a ghost.
#[derive(Debug, Clone)]
pub struct ActorView {
    pub name: String,
    /// The profile; `None` for a ghost.
    pub url: Option<String>,
    pub avatar: Avatar,
    pub is_ghost: bool,
}

impl ActorView {
    pub fn new(account: Option<&Account>) -> Self {
        match account {
            Some(account) => Self {
                name: account.name.to_string(),
                url: Some(format!("/{}", account.name)),
                avatar: Avatar::for_account(account),
                is_ghost: false,
            },
            None => Self::ghost(),
        }
    }

    pub fn ghost() -> Self {
        // Palette 5 is the neutral one (subtle background, muted ink).
        Self { name: "ghost".to_owned(), url: None, avatar: Avatar { initials: "?".to_owned(), palette: 5 }, is_ghost: true }
    }
}

/// One piece of an event sentence. `kind` tells `timeline/event.html` how to render it:
/// `text`, `strong`, `struck`, `code`, `link` (`url`), `code-link` (`url`), `label`
/// (`color`), `account` (`url`, none for a ghost) or `muted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub kind: &'static str,
    pub text: String,
    pub url: Option<String>,
    /// A validated `#rrggbb` (labels only), used as an SVG `fill`.
    pub color: Option<String>,
}

impl Fragment {
    fn new(kind: &'static str, text: impl Into<String>) -> Self {
        Self { kind, text: text.into(), url: None, color: None }
    }

    fn text(text: impl Into<String>) -> Self {
        Self::new("text", text)
    }

    fn strong(text: impl Into<String>) -> Self {
        Self::new("strong", text)
    }

    fn struck(text: impl Into<String>) -> Self {
        Self::new("struck", text)
    }

    fn code(text: impl Into<String>) -> Self {
        Self::new("code", text)
    }

    fn muted(text: impl Into<String>) -> Self {
        Self::new("muted", text)
    }

    fn link(text: impl Into<String>, url: String) -> Self {
        Self { url: Some(url), ..Self::new("link", text) }
    }

    fn commit(commit: &str, urls: &RepoUrls) -> Self {
        Self { url: Some(urls.commit(commit)), ..Self::new("code-link", short_sha(commit)) }
    }

    fn label(label: &LabelSnapshot) -> Self {
        Self { color: Some(valid_color(&label.color).unwrap_or("#888888").to_owned()), ..Self::new("label", label.name.clone()) }
    }

    fn account(account: Option<&Account>) -> Self {
        let actor = ActorView::new(account);
        Self { url: actor.url, ..Self::new("account", actor.name) }
    }

    /// The plain text, for tests and `title` attributes.
    pub fn plain(parts: &[Self]) -> String {
        parts.iter().map(|part| part.text.as_str()).collect()
    }
}

/// `color` if it is exactly `#rrggbb`.
fn valid_color(color: &str) -> Option<&str> {
    let hex = color.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(color)
}

/// The open / closed / merged / draft pill of a thread (`timeline/state_badge.html`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateBadge {
    /// `state-badge-open`, `-closed`, `-merged` or `-draft`.
    pub class: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
}

impl StateBadge {
    /// The badge of `issue` by kind, state and reason. P shows [`Self::draft`] for drafts.
    pub fn for_issue(issue: &Issue) -> Self {
        match (issue.kind, issue.state, issue.state_reason) {
            (IssueKind::Issue, IssueState::Open, _) => Self { class: "state-badge-open", icon: "circle-dot", label: "Open" },
            (IssueKind::Issue, IssueState::Closed, Some(StateReason::NotPlanned)) => {
                Self { class: "state-badge-closed", icon: "circle-slash", label: "Closed" }
            }
            (IssueKind::Issue, IssueState::Closed, _) => Self { class: "state-badge-closed", icon: "circle-check", label: "Closed" },
            (IssueKind::Pull, IssueState::Open, _) => Self { class: "state-badge-open", icon: "git-pull-request", label: "Open" },
            (IssueKind::Pull, IssueState::Closed, Some(StateReason::Merged)) => Self::merged(),
            (IssueKind::Pull, IssueState::Closed, _) => {
                Self { class: "state-badge-closed", icon: "git-pull-request-closed", label: "Closed" }
            }
        }
    }

    pub const fn merged() -> Self {
        Self { class: "state-badge-merged", icon: "git-merge", label: "Merged" }
    }

    pub const fn draft() -> Self {
        Self { class: "state-badge-draft", icon: "git-pull-request-draft", label: "Draft" }
    }
}

/// A comment, or the opening post (`timeline/comment.html`).
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct CommentView {
    pub anchor: String,
    pub author: ActorView,
    /// The comment is by whoever opened the thread ("Author" badge).
    pub by_thread_author: bool,
    /// Sanitised HTML; empty for an opening post without description.
    pub body_html: String,
    /// The Markdown source, for the edit form.
    pub source: String,
    pub when: String,
    pub when_iso: String,
    pub edited: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    /// Show the edit form instead of the text.
    pub editing: bool,
    /// `GET`: the page with the edit form open.
    pub edit_url: String,
    /// `GET`: back to the text.
    pub cancel_url: String,
    /// `POST` target of the edit form.
    pub update_url: String,
    /// `POST` target that deletes the comment; `None` for the opening post.
    pub delete_url: Option<String>,
    pub is_opening_post: bool,
    pub can_reply: bool,
}

/// An event row (`timeline/event.html`): "alice added the bug label 2 days ago".
#[derive(Debug, Clone)]
pub struct EventView {
    pub anchor: String,
    /// An icon `ui::icon` knows.
    pub icon: &'static str,
    pub actor: ActorView,
    /// What the actor did, after their name.
    pub sentence: Vec<Fragment>,
    /// A muted second line (a commit summary), if any.
    pub detail: Option<String>,
    pub when: String,
    pub when_iso: String,
}

/// One entry of the conversation (`timeline/item.html`): exactly one of the two is set.
#[derive(Debug, Clone)]
pub struct TimelineItemView {
    /// Its position, for interleaving other items (reviews) in order.
    pub cursor: TimelineCursor,
    pub comment: Option<CommentView>,
    pub event: Option<EventView>,
}

/// The title with its in-place edit form (`timeline/title_form.html`).
#[derive(Debug, Clone)]
pub struct TitleView {
    pub anchor: &'static str,
    pub title: String,
    /// `#12`.
    pub number: String,
    pub can_edit: bool,
    pub editing: bool,
    pub edit_url: String,
    pub cancel_url: String,
    pub update_url: String,
}

/// The form under the conversation (`timeline/comment_form.html`).
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct CommentFormView {
    pub anchor: &'static str,
    pub action_url: String,
    pub suggestions_url: String,
    pub signed_in: bool,
    pub sign_in_url: String,
    pub can_comment: bool,
    pub locked: bool,
    pub can_set_state: bool,
    pub is_open: bool,
    /// Plain issues can also be closed as not planned.
    pub is_issue: bool,
    /// "issue" or "pull request", for button labels.
    pub noun: &'static str,
    /// `POST` target of the state buttons shown when the viewer may not comment (the
    /// author of a locked thread).
    pub state_url: String,
    pub can_lock: bool,
    pub lock_url: String,
}

/// Everything T0 renders for one thread.
#[derive(Debug, Clone)]
pub struct ThreadView {
    pub urls: ThreadUrls,
    pub badge: StateBadge,
    pub title: TitleView,
    /// The description, shown like a comment (`timeline/comment.html`).
    pub opening: CommentView,
    pub items: Vec<TimelineItemView>,
    /// "Load more": the page continued after the last item.
    pub next_url: Option<String>,
    pub form: CommentFormView,
}

/// Page-specific choices for [`thread`].
#[derive(Debug, Clone, Copy, Default)]
pub struct TimelineOptions<'a> {
    /// The `?edit=` query value: `comment-{id}`, `body` or `title`.
    pub edit: Option<&'a str>,
    /// The pull request's head branch, named in push sentences (P).
    pub head_branch: Option<&'a str>,
    /// The pull request's base branch, named in merge sentences (P).
    pub base_branch: Option<&'a str>,
}

// ------------------------------------------------------------------ building

/// Loads one page of `ctx`'s timeline for `viewer` (after `after`), resolves the
/// references in every text with one `ReferenceService::resolve` call and builds the
/// view. Errors are the services'.
pub async fn thread(
    state: &AppState,
    ctx: &IssueContext,
    viewer: Option<&User>,
    after: Option<TimelineCursor>,
    options: TimelineOptions<'_>,
) -> Result<ThreadView, ServiceError> {
    let timeline = state.services.timeline.timeline(viewer, ctx, after, MAX_TIMELINE_PAGE).await?;
    let mut texts = vec![ctx.issue.body.as_str()];
    texts.extend(timeline.entries.iter().filter_map(|entry| match entry {
        TimelineEntry::Comment { comment, .. } => Some(comment.body.as_str()),
        TimelineEntry::Event { .. } => None,
    }));
    let refs = state.services.references.resolve(&ctx.repo, &texts).await?;
    let repo = &ctx.repo.repo;
    let urls = RepoUrls::new(repo.owner.name.as_str(), repo.repo.name.as_str());
    let refs = RefContext {
        base: DocumentBase::new(urls, repo.repo.default_branch.clone(), ""),
        owner: repo.owner.name.to_string(),
        repo: repo.repo.name.to_string(),
        refs,
    };
    Ok(build_thread(ctx, viewer, &timeline, &refs, options))
}

/// [`thread`] without I/O: the view of an already loaded timeline page.
pub fn build_thread(
    ctx: &IssueContext,
    viewer: Option<&User>,
    timeline: &Timeline,
    refs: &RefContext,
    options: TimelineOptions<'_>,
) -> ThreadView {
    let urls = ThreadUrls::for_context(ctx);
    let issue = &ctx.issue;
    let permissions = ctx.permissions;
    let opening = CommentView {
        anchor: BODY_ANCHOR.to_owned(),
        author: ActorView::new(ctx.author.as_ref()),
        by_thread_author: false,
        body_html: if issue.body.as_str().trim().is_empty() { String::new() } else { render_issue_markdown(issue.body.as_str(), refs) },
        source: issue.body.to_string(),
        when: issue.created_at.relative(),
        when_iso: issue.created_at.iso8601(),
        edited: false,
        can_edit: permissions.can_edit,
        can_delete: false,
        editing: permissions.can_edit && options.edit == Some("body"),
        edit_url: urls.editing("body", BODY_ANCHOR),
        cancel_url: urls.at(BODY_ANCHOR),
        update_url: urls.body.clone(),
        delete_url: None,
        is_opening_post: true,
        can_reply: permissions.can_comment,
    };
    let title = TitleView {
        anchor: TITLE_ANCHOR,
        title: issue.title.to_string(),
        number: issue.number.to_string(),
        can_edit: permissions.can_edit,
        editing: permissions.can_edit && options.edit == Some("title"),
        edit_url: urls.editing("title", TITLE_ANCHOR),
        cancel_url: urls.at(TITLE_ANCHOR),
        update_url: urls.title.clone(),
    };
    let form = CommentFormView {
        anchor: NEW_COMMENT_ANCHOR,
        action_url: urls.comments.clone(),
        suggestions_url: format!("/{}/{}/issues/suggestions", ctx.repo.repo.owner.name, ctx.repo.repo.repo.name),
        signed_in: viewer.is_some(),
        sign_in_url: format!("/login?next={}", urls.page),
        can_comment: permissions.can_comment,
        locked: issue.locked,
        can_set_state: permissions.can_set_state,
        is_open: issue.is_open(),
        is_issue: !issue.is_pull(),
        noun: if issue.is_pull() { "pull request" } else { "issue" },
        state_url: urls.state.clone(),
        can_lock: permissions.can_lock,
        lock_url: urls.lock.clone(),
    };
    let items = items(ctx, timeline, refs, options);
    let next_url = timeline.next.map(|next| format!("{}?after={}", urls.page, next.to_param()));
    ThreadView { badge: StateBadge::for_issue(issue), title, opening, items, next_url, form, urls }
}

/// The views of a timeline page's entries, in order.
pub fn items(ctx: &IssueContext, timeline: &Timeline, refs: &RefContext, options: TimelineOptions<'_>) -> Vec<TimelineItemView> {
    let urls = ThreadUrls::for_context(ctx);
    let repo = &ctx.repo.repo;
    let repo_urls = RepoUrls::new(repo.owner.name.as_str(), repo.repo.name.as_str());
    timeline
        .entries
        .iter()
        .map(|entry| match entry {
            TimelineEntry::Comment { comment, author, can_edit, can_delete } => {
                let anchor = comment_anchor(comment.id);
                let editing_key = format!("comment-{}", comment.id.0);
                TimelineItemView {
                    cursor: entry.cursor(),
                    comment: Some(CommentView {
                        author: ActorView::new(author.as_ref()),
                        by_thread_author: comment.author.is_some() && comment.author == ctx.issue.author,
                        body_html: render_issue_markdown(comment.body.as_str(), refs),
                        source: comment.body.to_string(),
                        when: comment.created_at.relative(),
                        when_iso: comment.created_at.iso8601(),
                        edited: comment.edited_at.is_some(),
                        can_edit: *can_edit,
                        can_delete: *can_delete,
                        editing: *can_edit && options.edit == Some(editing_key.as_str()),
                        edit_url: urls.editing(&editing_key, &anchor),
                        cancel_url: urls.at(&anchor),
                        update_url: urls.comment(comment.id),
                        delete_url: Some(urls.delete_comment(comment.id)),
                        is_opening_post: false,
                        can_reply: ctx.permissions.can_comment,
                        anchor,
                    }),
                    event: None,
                }
            }
            TimelineEntry::Event { id, actor, event, at, subjects } => TimelineItemView {
                cursor: entry.cursor(),
                comment: None,
                event: Some(event_view(*id, actor.as_ref(), event, *at, subjects, &repo_urls, ctx, options)),
            },
        })
        .collect()
}

#[expect(clippy::too_many_arguments, reason = "one event and everything its sentence may name")]
fn event_view(
    id: EventId,
    actor: Option<&Account>,
    event: &TimelineEvent,
    at: Timestamp,
    subjects: &EventSubjects,
    urls: &RepoUrls,
    ctx: &IssueContext,
    options: TimelineOptions<'_>,
) -> EventView {
    let (icon, sentence, detail) = sentence(actor, event, subjects, urls, ctx, options);
    EventView {
        anchor: event_anchor(id),
        icon,
        actor: actor.map_or_else(ActorView::ghost, |account| ActorView::new(Some(account))),
        sentence,
        detail,
        when: at.relative(),
        when_iso: at.iso8601(),
    }
}

/// The icon, the words after the actor's name and an optional detail line of `event`.
#[expect(clippy::too_many_lines, reason = "one arm per event kind, kept together so none is missed")]
fn sentence(
    actor: Option<&Account>,
    event: &TimelineEvent,
    subjects: &EventSubjects,
    urls: &RepoUrls,
    ctx: &IssueContext,
    options: TimelineOptions<'_>,
) -> (&'static str, Vec<Fragment>, Option<String>) {
    use Fragment as F;
    let repo = &ctx.repo.repo;
    let (owner, name) = (repo.owner.name.as_str(), repo.repo.name.as_str());
    let subject = || F::account(subjects.account.as_ref());
    let is_self = |account: rbit_core::AccountId| actor.is_some_and(|actor| actor.id == account);
    match event {
        TimelineEvent::Closed { reason, commit, pull } => {
            let icon = match reason {
                StateReason::Completed => "circle-check",
                StateReason::NotPlanned => "circle-slash",
                StateReason::Merged => "git-merge",
            };
            let mut parts = vec![F::text(match reason {
                StateReason::Completed => "closed this as completed",
                StateReason::NotPlanned => "closed this as not planned",
                StateReason::Merged => "closed this as merged",
            })];
            if let Some(commit) = commit {
                parts.extend([F::text(" in "), F::commit(commit, urls)]);
            } else if let Some(pull) = pull {
                parts.extend([F::text(" in "), F::link(pull.to_string(), issue_url(owner, name, IssueKind::Pull, *pull))]);
            }
            (icon, parts, None)
        }
        TimelineEvent::Reopened => ("circle-dot", vec![F::text("reopened this")], None),
        TimelineEvent::Renamed { from, to } => {
            ("pencil", vec![F::text("changed the title "), F::struck(from.clone()), F::text(" "), F::strong(to.clone())], None)
        }
        TimelineEvent::Locked => ("lock", vec![F::text("locked the conversation and limited it to collaborators")], None),
        TimelineEvent::Unlocked => ("unlock", vec![F::text("unlocked this conversation")], None),
        TimelineEvent::Labeled { label } => ("tag", vec![F::text("added the "), F::label(label), F::text(" label")], None),
        TimelineEvent::Unlabeled { label } => ("tag", vec![F::text("removed the "), F::label(label), F::text(" label")], None),
        TimelineEvent::Milestoned { title, .. } => {
            ("milestone", vec![F::text("added this to the "), F::strong(title.clone()), F::text(" milestone")], None)
        }
        TimelineEvent::Demilestoned { title, .. } => {
            ("milestone", vec![F::text("removed this from the "), F::strong(title.clone()), F::text(" milestone")], None)
        }
        TimelineEvent::Assigned { assignee } if is_self(*assignee) => ("user-plus", vec![F::text("self-assigned this")], None),
        TimelineEvent::Assigned { .. } => ("user-plus", vec![F::text("assigned "), subject()], None),
        TimelineEvent::Unassigned { assignee } if is_self(*assignee) => ("user", vec![F::text("removed their assignment")], None),
        TimelineEvent::Unassigned { .. } => ("user", vec![F::text("unassigned "), subject()], None),
        TimelineEvent::CommitReferenced { commit, summary } => (
            "commit",
            vec![F::text("referenced this in commit "), F::commit(commit, urls)],
            Some(summary.clone()).filter(|s| !s.is_empty()),
        ),
        TimelineEvent::CrossReferenced { .. } => {
            let parts = match &subjects.source {
                Some(source) => {
                    let (owner, name) = source.full_name.split_once('/').unwrap_or((source.full_name.as_str(), ""));
                    let noun = if source.kind == IssueKind::Pull { "pull request" } else { "issue" };
                    let reference = if source.full_name == repo.full_name() {
                        source.number.to_string()
                    } else {
                        format!("{}{}", source.full_name, source.number)
                    };
                    vec![
                        F::text(format!("mentioned this in {noun} ")),
                        F::link(format!("{} {reference}", source.title), issue_url(owner, name, source.kind, source.number)),
                    ]
                }
                // The service drops cross-references the viewer may not see; this only
                // happens for views built by hand.
                None => vec![F::text("mentioned this")],
            };
            ("link", parts, None)
        }
        TimelineEvent::HeadPushed { before, after, forced: true, .. } => {
            let mut parts = vec![F::text("force-pushed ")];
            if let Some(branch) = options.head_branch {
                parts.extend([F::text("the "), F::code(branch), F::text(" branch ")]);
            }
            parts.extend([F::text("from "), F::commit(before, urls), F::text(" to "), F::commit(after, urls)]);
            ("git-pull-request", parts, None)
        }
        TimelineEvent::HeadPushed { after, count, .. } => {
            let noun = if *count == 1 { "commit" } else { "commits" };
            let mut parts = vec![F::text(format!("added {count} {noun}"))];
            if let Some(branch) = options.head_branch {
                parts.extend([F::text(" to "), F::code(branch)]);
            }
            parts.extend([F::text(", now at "), F::commit(after, urls)]);
            ("commit", parts, None)
        }
        TimelineEvent::BaseChanged { from, to } => {
            ("branch", vec![F::text("changed the base branch from "), F::code(from.clone()), F::text(" to "), F::code(to.clone())], None)
        }
        TimelineEvent::Merged { commit, strategy } => {
            let mut parts = vec![F::text("merged commit "), F::commit(commit, urls)];
            if let Some(base) = options.base_branch {
                parts.extend([F::text(" into "), F::code(base)]);
            }
            match strategy {
                Some(MergeStrategy::Merge) => parts.push(F::muted(" (merge commit)")),
                Some(MergeStrategy::Squash) => parts.push(F::muted(" (squash)")),
                Some(MergeStrategy::Rebase) => parts.push(F::muted(" (rebase)")),
                None => parts.push(F::muted(" (outside this instance)")),
            }
            ("git-merge", parts, None)
        }
        TimelineEvent::ReviewRequested { reviewer } if is_self(*reviewer) => {
            ("eye", vec![F::text("requested a review from themselves")], None)
        }
        TimelineEvent::ReviewRequested { .. } => ("eye", vec![F::text("requested a review from "), subject()], None),
        TimelineEvent::ReviewRequestRemoved { .. } => ("eye-off", vec![F::text("removed the request for review from "), subject()], None),
        TimelineEvent::ReviewDismissed { review, reviewer } => {
            let link = F::link("review", format!("#pullrequestreview-{}", review.0));
            let parts = match reviewer {
                Some(_) => vec![F::text("dismissed "), subject(), F::text("’s "), link],
                None => vec![F::text("dismissed a "), link],
            };
            ("x", parts, None)
        }
        TimelineEvent::ReviewThreadResolved { comment } | TimelineEvent::ReviewThreadReopened { comment } => {
            let verb = if matches!(event, TimelineEvent::ReviewThreadResolved { .. }) { "resolved " } else { "reopened " };
            ("message-square", vec![F::text(verb), F::link("a review conversation", format!("#r{}", comment.0))], None)
        }
        TimelineEvent::HeadRefDeleted { branch } => {
            ("trash", vec![F::text("deleted the "), F::code(branch.clone()), F::text(" branch")], None)
        }
        TimelineEvent::ConvertedToDraft => ("git-pull-request-draft", vec![F::text("marked this pull request as draft")], None),
        TimelineEvent::ReadyForReview => ("eye", vec![F::text("marked this pull request as ready for review")], None),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use askama::Template;
    use rbit_core::{
        AccountId, AccountKind, Comment, IssueId, IssuePermissions, IssueText, IssueTitle, LabelId, RepoAccess, RepoId, Repository,
        ReviewId, Visibility,
        issue::{CommentId as Cid, MilestoneId},
        repository::OwnedRepository,
    };
    use rbit_git::RepositoryStorage;
    use rbit_services::{
        RepoContext,
        references::ReferenceSet,
        timeline::{CrossReferenceSource, TimelineItemId},
    };

    use super::*;

    fn account(id: i64, name: &str) -> Account {
        Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: name.parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        }
    }

    fn context(kind: IssueKind, permissions: IssuePermissions) -> IssueContext {
        let owner = account(1, "alice");
        let repo = Repository {
            id: RepoId(1),
            owner_id: owner.id,
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
            pushed_at: None,
            archived_at: None,
        };
        let storage = RepositoryStorage::new(PathBuf::from("unused"));
        let repo = RepoContext {
            reader: storage.reader(owner.id, repo.id),
            disk_path: storage.path_for(owner.id, repo.id),
            repo: OwnedRepository { owner: owner.clone(), repo },
            access: RepoAccess::Write,
        };
        let issue = Issue {
            id: IssueId(5),
            repo_id: RepoId(1),
            number: IssueNumber::new(12).unwrap(),
            kind,
            title: IssueTitle::parse("Crash <on> start").unwrap(),
            body: IssueText::body("Steps: **run** it").unwrap(),
            author: Some(owner.id),
            state: IssueState::Open,
            state_reason: None,
            closed_at: None,
            closed_by: None,
            locked: false,
            comment_count: 1,
            created_at: Timestamp(100),
            updated_at: Timestamp(100),
        };
        IssueContext { repo, issue, author: Some(owner), permissions }
    }

    fn all() -> IssuePermissions {
        IssuePermissions {
            can_comment: true,
            can_edit: true,
            can_set_state: true,
            can_lock: true,
            can_triage: true,
            can_delete: true,
            can_moderate: true,
        }
    }

    fn refs() -> RefContext {
        RefContext {
            base: DocumentBase::new(RepoUrls::new("alice", "demo"), "main", ""),
            owner: "alice".to_owned(),
            repo: "demo".to_owned(),
            refs: ReferenceSet::default(),
        }
    }

    fn comment(id: i64, author: Option<&Account>, body: &str) -> TimelineEntry {
        TimelineEntry::Comment {
            comment: Comment {
                id: Cid(id),
                issue_id: IssueId(5),
                author: author.map(|a| a.id),
                body: IssueText::comment(body).unwrap(),
                created_at: Timestamp(200),
                updated_at: Timestamp(200),
                edited_at: None,
            },
            author: author.cloned(),
            can_edit: true,
            can_delete: true,
        }
    }

    fn event(id: i64, actor: Option<&Account>, event: TimelineEvent, subjects: EventSubjects) -> TimelineEntry {
        TimelineEntry::Event { id: EventId(id), actor: actor.cloned(), event, at: Timestamp(300), subjects }
    }

    #[derive(Template)]
    #[template(source = r#"{% include "timeline/thread.html" %}"#, ext = "html")]
    struct ThreadFixture {
        thread: ThreadView,
    }

    #[derive(Template)]
    #[template(source = r#"{% include "timeline/title_form.html" %}{% include "timeline/state_badge.html" %}"#, ext = "html")]
    struct HeaderFixture {
        title: TitleView,
        badge: StateBadge,
    }

    fn sentence_of(
        ctx: &IssueContext,
        actor: Option<&Account>,
        timeline_event: TimelineEvent,
        subjects: EventSubjects,
        options: TimelineOptions<'_>,
    ) -> (String, String) {
        let timeline = Timeline { entries: vec![event(1, actor, timeline_event, subjects)], next: None };
        let item = items(ctx, &timeline, &refs(), options).remove(0);
        let view = item.event.unwrap();
        (view.icon.to_owned(), Fragment::plain(&view.sentence))
    }

    #[test]
    fn urls_follow_the_kind_and_forms_share_the_issue_path() {
        let number = IssueNumber::new(7).unwrap();
        let pull = ThreadUrls::new("alice", "demo", IssueKind::Pull, number);
        assert_eq!(pull.page, "/alice/demo/pull/7");
        assert_eq!(pull.comments, "/alice/demo/issues/7/comments");
        assert_eq!(pull.comment(Cid(3)), "/alice/demo/issues/7/comments/3");
        assert_eq!(pull.delete_comment(Cid(3)), "/alice/demo/issues/7/comments/3/delete");
        assert_eq!((pull.title.as_str(), pull.body.as_str()), ("/alice/demo/issues/7/title", "/alice/demo/issues/7/body"));
        assert_eq!((pull.state.as_str(), pull.lock.as_str()), ("/alice/demo/issues/7/state", "/alice/demo/issues/7/lock"));
        assert_eq!(ThreadUrls::new("alice", "demo", IssueKind::Issue, number).page, "/alice/demo/issues/7");
        assert_eq!(comment_anchor(Cid(9)), "issuecomment-9");
        assert_eq!(event_anchor(EventId(4)), "event-4");
    }

    #[test]
    fn state_badges() {
        let mut issue = context(IssueKind::Issue, all()).issue;
        assert_eq!(StateBadge::for_issue(&issue).class, "state-badge-open");
        issue.state = IssueState::Closed;
        issue.state_reason = Some(StateReason::NotPlanned);
        assert_eq!(StateBadge::for_issue(&issue).icon, "circle-slash");
        issue.state_reason = Some(StateReason::Completed);
        assert_eq!((StateBadge::for_issue(&issue).icon, StateBadge::for_issue(&issue).label), ("circle-check", "Closed"));
        issue.kind = IssueKind::Pull;
        assert_eq!(StateBadge::for_issue(&issue).icon, "git-pull-request-closed");
        issue.state_reason = Some(StateReason::Merged);
        assert_eq!(StateBadge::for_issue(&issue), StateBadge::merged());
        issue.state = IssueState::Open;
        issue.state_reason = None;
        assert_eq!(StateBadge::for_issue(&issue).icon, "git-pull-request");
        assert_eq!(StateBadge::draft().class, "state-badge-draft");
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "one case per event kind")]
    fn every_event_kind_has_a_sentence() {
        let ctx = context(IssueKind::Pull, all());
        let alice = account(1, "alice");
        let bob = account(2, "bob");
        let none = EventSubjects::default;
        let bob_subject = || EventSubjects { account: Some(bob.clone()), source: None };
        let label = LabelSnapshot { id: LabelId(1), name: "bug".to_owned(), color: "#d73a4a".to_owned() };
        let branches = TimelineOptions { edit: None, head_branch: Some("feature"), base_branch: Some("main") };
        let plain = TimelineOptions::default();
        let a = Some(&alice);
        let cases: Vec<(TimelineEvent, EventSubjects, TimelineOptions<'_>, &str, &str)> = vec![
            (
                TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: None },
                none(),
                plain,
                "circle-check",
                "closed this as completed",
            ),
            (
                TimelineEvent::Closed { reason: StateReason::NotPlanned, commit: None, pull: None },
                none(),
                plain,
                "circle-slash",
                "closed this as not planned",
            ),
            (
                TimelineEvent::Closed { reason: StateReason::Completed, commit: Some("abc1234def".to_owned()), pull: None },
                none(),
                plain,
                "circle-check",
                "closed this as completed in abc1234",
            ),
            (
                TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: IssueNumber::new(2) },
                none(),
                plain,
                "circle-check",
                "closed this as completed in #2",
            ),
            (
                TimelineEvent::Closed { reason: StateReason::Merged, commit: None, pull: None },
                none(),
                plain,
                "git-merge",
                "closed this as merged",
            ),
            (TimelineEvent::Reopened, none(), plain, "circle-dot", "reopened this"),
            (TimelineEvent::Renamed { from: "Old".to_owned(), to: "New".to_owned() }, none(), plain, "pencil", "changed the title Old New"),
            (TimelineEvent::Locked, none(), plain, "lock", "locked the conversation and limited it to collaborators"),
            (TimelineEvent::Unlocked, none(), plain, "unlock", "unlocked this conversation"),
            (TimelineEvent::Labeled { label: label.clone() }, none(), plain, "tag", "added the bug label"),
            (TimelineEvent::Unlabeled { label }, none(), plain, "tag", "removed the bug label"),
            (
                TimelineEvent::Milestoned { milestone: MilestoneId(1), title: "v1".to_owned() },
                none(),
                plain,
                "milestone",
                "added this to the v1 milestone",
            ),
            (
                TimelineEvent::Demilestoned { milestone: MilestoneId(1), title: "v1".to_owned() },
                none(),
                plain,
                "milestone",
                "removed this from the v1 milestone",
            ),
            (TimelineEvent::Assigned { assignee: bob.id }, bob_subject(), plain, "user-plus", "assigned bob"),
            (TimelineEvent::Assigned { assignee: alice.id }, none(), plain, "user-plus", "self-assigned this"),
            (TimelineEvent::Unassigned { assignee: bob.id }, bob_subject(), plain, "user", "unassigned bob"),
            (TimelineEvent::Unassigned { assignee: alice.id }, none(), plain, "user", "removed their assignment"),
            (TimelineEvent::Assigned { assignee: AccountId(99) }, none(), plain, "user-plus", "assigned ghost"),
            (
                TimelineEvent::CommitReferenced { commit: "abc1234def".to_owned(), summary: "Fix it".to_owned() },
                none(),
                plain,
                "commit",
                "referenced this in commit abc1234",
            ),
            (
                TimelineEvent::CrossReferenced { source: IssueId(8) },
                EventSubjects {
                    account: None,
                    source: Some(CrossReferenceSource {
                        full_name: "carol/other".to_owned(),
                        number: IssueNumber::new(3).unwrap(),
                        kind: IssueKind::Issue,
                        state: IssueState::Open,
                        title: "Related".to_owned(),
                    }),
                },
                plain,
                "link",
                "mentioned this in issue Related carol/other#3",
            ),
            (
                TimelineEvent::HeadPushed { before: "abc1234aaa".to_owned(), after: "def5678bbb".to_owned(), count: 2, forced: true },
                none(),
                branches,
                "git-pull-request",
                "force-pushed the feature branch from abc1234 to def5678",
            ),
            (
                TimelineEvent::HeadPushed { before: "abc1234aaa".to_owned(), after: "def5678bbb".to_owned(), count: 1, forced: false },
                none(),
                branches,
                "commit",
                "added 1 commit to feature, now at def5678",
            ),
            (
                TimelineEvent::BaseChanged { from: "main".to_owned(), to: "dev".to_owned() },
                none(),
                plain,
                "branch",
                "changed the base branch from main to dev",
            ),
            (
                TimelineEvent::Merged { commit: "abc1234fff".to_owned(), strategy: Some(MergeStrategy::Squash) },
                none(),
                branches,
                "git-merge",
                "merged commit abc1234 into main (squash)",
            ),
            (
                TimelineEvent::Merged { commit: "abc1234fff".to_owned(), strategy: None },
                none(),
                plain,
                "git-merge",
                "merged commit abc1234 (outside this instance)",
            ),
            (TimelineEvent::ReviewRequested { reviewer: bob.id }, bob_subject(), plain, "eye", "requested a review from bob"),
            (
                TimelineEvent::ReviewRequestRemoved { reviewer: bob.id },
                bob_subject(),
                plain,
                "eye-off",
                "removed the request for review from bob",
            ),
            (
                TimelineEvent::ReviewDismissed { review: ReviewId(4), reviewer: Some(bob.id) },
                bob_subject(),
                plain,
                "x",
                "dismissed bob’s review",
            ),
            (TimelineEvent::ReviewDismissed { review: ReviewId(4), reviewer: None }, none(), plain, "x", "dismissed a review"),
            (
                TimelineEvent::ReviewThreadResolved { comment: rbit_core::pull::ReviewCommentId(9) },
                none(),
                plain,
                "message-square",
                "resolved a review conversation",
            ),
            (
                TimelineEvent::ReviewThreadReopened { comment: rbit_core::pull::ReviewCommentId(9) },
                none(),
                plain,
                "message-square",
                "reopened a review conversation",
            ),
            (TimelineEvent::HeadRefDeleted { branch: "feature".to_owned() }, none(), plain, "trash", "deleted the feature branch"),
            (TimelineEvent::ConvertedToDraft, none(), plain, "git-pull-request-draft", "marked this pull request as draft"),
            (TimelineEvent::ReadyForReview, none(), plain, "eye", "marked this pull request as ready for review"),
        ];
        let mut kinds = std::collections::HashSet::new();
        for (timeline_event, subjects, options, icon, expected) in cases {
            kinds.insert(timeline_event.kind());
            let (actual_icon, sentence) = sentence_of(&ctx, a, timeline_event, subjects, options);
            assert_eq!((actual_icon.as_str(), sentence.as_str()), (icon, expected));
        }
        assert_eq!(kinds.len(), 24, "every TimelineEvent kind is covered");
    }

    #[test]
    fn the_thread_renders_comments_events_and_forms() {
        let ctx = context(IssueKind::Issue, all());
        let alice = account(1, "alice");
        let label = LabelSnapshot { id: LabelId(1), name: "bug".to_owned(), color: "#d73a4a\" onload=\"x".to_owned() };
        let timeline = Timeline {
            entries: vec![
                comment(7, Some(&alice), "Hello <script>alert(1)</script> **world**"),
                event(3, Some(&alice), TimelineEvent::Labeled { label }, EventSubjects::default()),
                comment(8, None, "From a deleted account"),
            ],
            next: Some(TimelineCursor { at: Timestamp(400), item: TimelineItemId::Event(EventId(3)) }),
        };
        let thread = build_thread(
            &ctx,
            Some(&User { account: alice.clone(), email: "a@example.com".parse().unwrap(), is_admin: false }),
            &timeline,
            &refs(),
            TimelineOptions::default(),
        );
        assert_eq!(thread.items.len(), 3);
        assert_eq!(thread.items[0].cursor.item, TimelineItemId::Comment(Cid(7)));
        let html = ThreadFixture { thread }.render().unwrap();

        assert!(html.contains(r#"id="issue-body""#) && html.contains("<strong>run</strong>"), "the description is Markdown");
        assert!(html.contains(r#"id="issuecomment-7""#) && html.contains("<strong>world</strong>"));
        assert!(!html.contains("<script>alert"), "comment HTML is sanitised");
        assert!(html.contains(r#"<span class="badge">Author</span>"#), "the thread author's comments are marked");
        assert!(html.contains(r#"id="event-3""#) && html.contains("added the "));
        assert!(html.contains(r##"fill="#888888""##), "an invalid label colour never reaches the attribute");
        assert!(!html.contains("onload"));
        assert!(html.contains(r#"<span class="font-medium text-ink-muted">ghost</span>"#), "deleted accounts are a ghost without a link");
        assert!(html.contains(r#"action="/alice/demo/issues/12/comments/7/delete""#));
        assert!(html.contains(r#"href="/alice/demo/issues/12?edit=comment-7#issuecomment-7""#));
        assert!(html.contains(r##"hx-select="#issuecomment-7""##));
        assert!(html.contains(r#"href="/alice/demo/issues/12?after=400-e3""#), "load more");
        assert!(html.contains(r#"id="new-comment""#) && html.contains(r#"action="/alice/demo/issues/12/comments""#));
        assert!(html.contains(r#"value="close_completed""#) && html.contains(r#"value="close_not_planned""#));
        assert!(html.contains("Lock conversation") && html.contains(r#"action="/alice/demo/issues/12/lock""#));
        assert!(!html.contains("style="), "no inline styles under the CSP");
    }

    #[test]
    fn edit_forms_open_in_place_only_for_editors() {
        let ctx = context(IssueKind::Pull, all());
        let alice = account(1, "alice");
        let timeline = Timeline { entries: vec![comment(7, Some(&alice), "Typo")], next: None };
        let options = TimelineOptions { edit: Some("comment-7"), ..TimelineOptions::default() };
        let html = ThreadFixture { thread: build_thread(&ctx, None, &timeline, &refs(), options) }.render().unwrap();
        assert!(html.contains(r#"action="/alice/demo/issues/12/comments/7""#) && html.contains(">Typo</textarea>"));
        assert!(html.contains(r#"href="/alice/demo/pull/12#issuecomment-7""#), "cancel returns to the pull request page");
        assert!(!html.contains(r#"value="close_not_planned""#), "pull requests are not closed as not planned");
        assert!(html.contains("Close pull request"));

        let options = TimelineOptions { edit: Some("body"), ..TimelineOptions::default() };
        let html = ThreadFixture { thread: build_thread(&ctx, None, &timeline, &refs(), options) }.render().unwrap();
        assert!(html.contains(r#"action="/alice/demo/issues/12/body""#) && html.contains(">Steps: **run** it</textarea>"));

        let reader = IssuePermissions { can_comment: true, ..IssuePermissions::default() };
        let mut ctx = context(IssueKind::Issue, reader);
        let timeline = Timeline {
            entries: vec![match comment(7, Some(&alice), "Typo") {
                TimelineEntry::Comment { comment, author, .. } => {
                    TimelineEntry::Comment { comment, author, can_edit: false, can_delete: false }
                }
                other @ TimelineEntry::Event { .. } => other,
            }],
            next: None,
        };
        let html = ThreadFixture { thread: build_thread(&ctx, None, &timeline, &refs(), options) }.render().unwrap();
        assert!(!html.contains("</textarea>\n") || !html.contains(r#"action="/alice/demo/issues/12/body""#), "no body editor");
        assert!(!html.contains(">Edit</a>") && !html.contains("Delete comment") && !html.contains("Lock conversation"));
        assert!(!html.contains(r#"value="close_completed""#));

        ctx.issue.locked = true;
        ctx.permissions = IssuePermissions { can_set_state: true, ..IssuePermissions::default() };
        let viewer = User { account: alice.clone(), email: "a@example.com".parse().unwrap(), is_admin: false };
        let html =
            ThreadFixture { thread: build_thread(&ctx, Some(&viewer), &timeline, &refs(), TimelineOptions::default()) }.render().unwrap();
        assert!(html.contains("This conversation is locked") && html.contains(r#"action="/alice/demo/issues/12/state""#));
        assert!(!html.contains("Sign in"), "a signed-in author of a locked thread can still close it");
    }

    #[test]
    fn titles_and_badges_render() {
        let ctx = context(IssueKind::Issue, all());
        let thread = build_thread(&ctx, None, &Timeline::default(), &refs(), TimelineOptions::default());
        let html = HeaderFixture { title: thread.title.clone(), badge: thread.badge }.render().unwrap();
        assert!(html.contains("Crash &#60;on&#62; start") || html.contains("Crash &lt;on&gt; start"), "titles are escaped");
        assert!(html.contains("#12") && html.contains(r#"href="/alice/demo/issues/12?edit=title#issue-title""#));
        assert!(html.contains(r#"class="state-badge state-badge-open""#) && html.contains("Open</span>"));

        let options = TimelineOptions { edit: Some("title"), ..TimelineOptions::default() };
        let editing = build_thread(&ctx, None, &Timeline::default(), &refs(), options).title;
        let html = HeaderFixture { title: editing, badge: thread.badge }.render().unwrap();
        assert!(html.contains(r#"action="/alice/demo/issues/12/title""#) && html.contains(r#"name="title""#));
    }

    #[test]
    fn anonymous_viewers_are_invited_to_sign_in() {
        let ctx = context(IssueKind::Issue, IssuePermissions::default());
        let thread = build_thread(&ctx, None, &Timeline::default(), &refs(), TimelineOptions::default());
        let html = ThreadFixture { thread }.render().unwrap();
        assert!(html.contains(r#"href="/login?next=/alice/demo/issues/12""#));
        assert!(!html.contains(r#"name="body""#));
    }
}
