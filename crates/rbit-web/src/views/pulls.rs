//! Pull request list and pages: conversation, commits, files changed, reviews and the merge box.
//!
//! Milestone 2 workstream P (pull requests) owns this file and `templates/pulls/**`.
//!
//! The conversation interleaves T0's timeline items (`timeline/item.html`) with this
//! module's reviews by time ([`ConversationEntry`]); each review is anchored
//! `#pullrequestreview-{id}` (T0's `ReviewDismissed` sentence links there) and each review
//! comment `#r{id}`. Every line of the Files changed tab has the stable anchor
//! `#diff-{sha256(path)[..8]}-{L|R}{line}` ([`line_anchor`]); `?comment=<anchor>` opens the
//! inline comment form there without JavaScript.

use std::collections::HashMap;

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Issue, IssueState, User,
    issue::{MergeStrategy, ReviewState},
    pull::{Mergeability, ReviewComment, ReviewSide},
};
use rbit_git::{
    ChangeStatus, FileDiff, FileDiffBody,
    diff::{Hunk, LineKind},
};
use rbit_services::pulls::{MergeBox, PullContext, PullListItem, ReviewData, ReviewEntry};
use sha2::{Digest, Sha256};

use crate::{
    render::references::{RefContext, render_issue_markdown},
    urls::{RepoUrls, encode_path},
    views::{
        blocks::{Block, Blocker},
        // `DiffBodyView` is matched by `repo/file_diffs.html` (the new pull request page).
        commit::{DiffBodyView, DiffLineView, width_classes},
        compare::ComparePicker,
        components::Pagination,
        issues::IssueSidebar,
        layout::Page,
        repo::{CommitView, RepoHeader},
        timeline::{ActorView, StateBadge, ThreadView, TimelineItemView},
    },
};

// ------------------------------------------------------------------ anchors and URLs

/// `diff-{first 8 hex digits of sha256(path)}`: a file's anchor on the Files changed tab.
pub fn file_anchor(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let hex = digest.iter().take(4).fold(String::with_capacity(8), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("diff-{hex}")
}

/// `diff-{hash}-L{line}` or `-R{line}`: one line's anchor.
pub fn line_anchor(path: &str, side: ReviewSide, line: u32) -> String {
    let letter = match side {
        ReviewSide::Left => 'L',
        ReviewSide::Right => 'R',
    };
    format!("{}-{letter}{line}", file_anchor(path))
}

/// `pullrequestreview-{id}`.
pub fn review_anchor(id: i64) -> String {
    format!("pullrequestreview-{id}")
}

/// `r{id}`: a review comment.
pub fn review_comment_anchor(id: i64) -> String {
    format!("r{id}")
}

/// The pages and forms of one pull request.
#[derive(Debug, Clone)]
pub struct PullUrls {
    pub page: String,
    pub commits: String,
    pub files: String,
    pub merge: String,
    pub draft: String,
    pub line_comments: String,
    pub reviews: String,
    pub discard_review: String,
    pub reviewers: String,
    pub remove_reviewer: String,
    pub delete_branch: String,
}

impl PullUrls {
    pub fn new(owner: &str, repo: &str, number: u32) -> Self {
        let page = format!("/{owner}/{repo}/pull/{number}");
        Self {
            commits: format!("{page}/commits"),
            files: format!("{page}/files"),
            merge: format!("{page}/merge"),
            draft: format!("{page}/draft"),
            line_comments: format!("{page}/comments"),
            reviews: format!("{page}/reviews"),
            discard_review: format!("{page}/reviews/discard"),
            reviewers: format!("{page}/reviewers"),
            remove_reviewer: format!("{page}/reviewers/remove"),
            delete_branch: format!("{page}/branch/delete"),
            page,
        }
    }

    pub fn for_context(ctx: &PullContext) -> Self {
        let repo = &ctx.repo().repo;
        Self::new(repo.owner.name.as_str(), repo.repo.name.as_str(), ctx.issue().number.get())
    }

    /// `POST`: dismiss review `id`.
    pub fn dismiss(&self, id: i64) -> String {
        format!("{}/reviews/{id}/dismiss", self.page)
    }
}

// ------------------------------------------------------------------ header and tabs

/// A lazily filled count in the pull request header. The header and
/// [`PullCountsFragment`] both take the element ids from [`count_slot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountSlot {
    /// "3 commits" in the "wants to merge" sentence.
    CommitsPhrase,
    /// The Commits tab's badge.
    Commits,
    /// The Files changed tab's badge.
    Files,
}

/// The DOM id of a header count slot.
pub const fn count_slot(slot: CountSlot) -> &'static str {
    match slot {
        CountSlot::CommitsPhrase => "pull-commits-phrase",
        CountSlot::Commits => "pull-commit-count",
        CountSlot::Files => "pull-file-count",
    }
}

/// The header's counts; `None` until a deferred request fills them in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PullCountsView {
    pub commits: Option<usize>,
    pub files: Option<usize>,
}

impl PullCountsView {
    pub fn known(commits: usize, files: usize) -> Self {
        Self { commits: Some(commits), files: Some(files) }
    }

    /// "1 commit", "3 commits", or just "commits" while unknown (also the no-JS text).
    pub fn commits_phrase(&self) -> String {
        match self.commits {
            Some(1) => "1 commit".to_owned(),
            Some(count) => format!("{count} commits"),
            None => "commits".to_owned(),
        }
    }

    /// A badge's text: the count, or a dash while unknown.
    fn badge(count: Option<usize>) -> String {
        count.map_or_else(|| "\u{2013}".to_owned(), |count| count.to_string())
    }

    pub fn commits_badge(&self) -> String {
        Self::badge(self.commits)
    }

    pub fn files_badge(&self) -> String {
        Self::badge(self.files)
    }
}

/// Title, state and "alice wants to merge 3 commits into `main` from `alice:feature`".
#[derive(Debug, Clone)]
pub struct PullHeaderView {
    pub title: String,
    pub number: String,
    pub badge: StateBadge,
    pub author: ActorView,
    /// "wants to merge", "merged" or "proposed to merge".
    pub verb: &'static str,
    pub base_branch: String,
    pub base_url: String,
    pub head_label: String,
    pub head_url: Option<String>,
    pub urls: PullUrls,
    /// `conversation`, `commits` or `files`.
    pub active: &'static str,
    pub counts: PullCountsView,
    /// Where missing counts come from ([`PullCountsFragment`]); `None` when all are known.
    pub counts_url: Option<String>,
    pub is_draft: bool,
}

impl PullHeaderView {
    pub fn new(ctx: &PullContext, counts: PullCountsView, counts_url: Option<String>, active: &'static str) -> Self {
        let issue = ctx.issue();
        let repo = &ctx.repo().repo;
        let urls = RepoUrls::new(repo.owner.name.as_str(), repo.repo.name.as_str());
        let badge = badge(issue, ctx.pull.is_draft, ctx.pull.is_merged());
        let verb = match (ctx.pull.is_merged(), issue.state) {
            (true, _) => "merged",
            (false, IssueState::Open) => "wants to merge",
            (false, IssueState::Closed) => "proposed to merge",
        };
        Self {
            title: issue.title.to_string(),
            number: issue.number.to_string(),
            badge,
            author: ActorView::new(ctx.thread.author.as_ref()),
            verb,
            base_url: urls.tree(&ctx.pull.base_branch, ""),
            base_branch: ctx.pull.base_branch.clone(),
            head_label: ctx.head_label(),
            head_url: ctx
                .head_repo
                .as_ref()
                .map(|head| RepoUrls::new(head.owner.name.as_str(), head.repo.name.as_str()).tree(&ctx.pull.head_branch, "")),
            urls: PullUrls::for_context(ctx),
            active,
            counts,
            counts_url,
            is_draft: ctx.pull.is_draft,
        }
    }

    /// For templates: the id of a count slot.
    pub fn slot(&self, slot: CountSlot) -> &'static str {
        count_slot(slot)
    }
}

/// `GET …/pull/{n}/counts/{base}...{head}`: the header counts as out-of-band swaps into
/// the slots of `pulls/header.html`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/counts.html")]
pub struct PullCountsFragment {
    pub counts: PullCountsView,
}

impl PullCountsFragment {
    /// For templates: the id of a count slot.
    pub fn slot(&self, slot: CountSlot) -> &'static str {
        count_slot(slot)
    }
}

/// The state pill of a pull request: Draft for open drafts, Merged once merged.
pub fn badge(issue: &Issue, is_draft: bool, is_merged: bool) -> StateBadge {
    if is_merged {
        StateBadge::merged()
    } else if is_draft && issue.state == IssueState::Open {
        StateBadge::draft()
    } else {
        StateBadge::for_issue(issue)
    }
}

// ------------------------------------------------------------------ reviews

/// A review comment as shown in the conversation or under a diff line.
#[derive(Debug, Clone)]
pub struct ReviewCommentView {
    pub anchor: String,
    pub author: ActorView,
    pub body_html: String,
    pub when: String,
    pub when_iso: String,
    pub path: String,
    /// `L12` / `R12`.
    pub line_label: String,
    /// The stored diff excerpt, one `(css kind, text)` per line after the header (app.css
    /// draws the sign from the kind).
    pub hunk_header: String,
    pub hunk: Vec<(&'static str, String)>,
    pub outdated: bool,
    pub pending: bool,
    pub is_reply: bool,
    /// Where the comment's line is on the Files changed tab.
    pub line_url: String,
    /// `POST` target of the reply form.
    pub reply_to: i64,
}

impl ReviewCommentView {
    pub fn new(comment: &ReviewComment, author: Option<&rbit_core::Account>, pending: bool, urls: &PullUrls, refs: &RefContext) -> Self {
        let mut lines = comment.diff_hunk.lines();
        let hunk_header = lines.next().unwrap_or_default().to_owned();
        let hunk = lines
            .map(|line| {
                let kind = match line.chars().next() {
                    Some('+') => "add",
                    Some('-') => "del",
                    _ => "ctx",
                };
                (kind, line.get(1..).unwrap_or_default().to_owned())
            })
            .collect();
        let side = match comment.side {
            ReviewSide::Left => 'L',
            ReviewSide::Right => 'R',
        };
        Self {
            anchor: review_comment_anchor(comment.id.0),
            author: ActorView::new(author),
            body_html: render_issue_markdown(&comment.body, refs),
            when: comment.created_at.relative(),
            when_iso: comment.created_at.iso8601(),
            path: comment.path.clone(),
            line_label: format!("{side}{}", comment.line),
            hunk_header,
            hunk,
            outdated: comment.outdated,
            pending,
            is_reply: comment.in_reply_to.is_some(),
            line_url: format!("{}#{}", urls.files, line_anchor(&comment.path, comment.side, comment.line)),
            reply_to: comment.in_reply_to.unwrap_or(comment.id).0,
        }
    }
}

/// A submitted review in the conversation.
#[derive(Debug, Clone)]
pub struct ReviewView {
    pub anchor: String,
    pub id: i64,
    pub reviewer: ActorView,
    /// "approved these changes", "requested changes", "reviewed", …
    pub sentence: &'static str,
    pub icon: &'static str,
    /// `text-success`, `text-danger` or `text-ink-muted`.
    pub tone: &'static str,
    pub body_html: String,
    pub when: String,
    pub when_iso: String,
    pub comments: Vec<ReviewCommentView>,
    pub dismissal: Option<String>,
    pub can_dismiss: bool,
    pub dismiss_url: String,
}

impl ReviewView {
    pub fn new(entry: &ReviewEntry, can_dismiss: bool, urls: &PullUrls, refs: &RefContext) -> Self {
        let review = &entry.review;
        let (sentence, icon, tone) = match review.state {
            ReviewState::Approved => ("approved these changes", "circle-check", "text-success"),
            ReviewState::ChangesRequested => ("requested changes", "file-diff", "text-danger"),
            ReviewState::Dismissed => ("reviewed (dismissed)", "eye-off", "text-ink-muted"),
            ReviewState::Commented | ReviewState::Pending => ("reviewed", "eye", "text-ink-muted"),
        };
        let at = review.submitted_at.unwrap_or(review.created_at);
        let dismissal = review.dismissal_message.as_ref().map(|message| {
            let by = entry.dismissed_by.as_ref().map_or_else(|| "ghost".to_owned(), |account| account.name.to_string());
            format!("Dismissed by {by}: {message}")
        });
        Self {
            anchor: review_anchor(review.id.0),
            id: review.id.0,
            reviewer: ActorView::new(entry.reviewer.as_ref()),
            sentence,
            icon,
            tone,
            body_html: if review.body.trim().is_empty() { String::new() } else { render_issue_markdown(&review.body, refs) },
            when: at.relative(),
            when_iso: at.iso8601(),
            comments: entry
                .comments
                .iter()
                .map(|(comment, author)| ReviewCommentView::new(comment, author.as_ref(), false, urls, refs))
                .collect(),
            dismissal,
            can_dismiss: can_dismiss && matches!(review.state, ReviewState::Approved | ReviewState::ChangesRequested),
            dismiss_url: urls.dismiss(review.id.0),
        }
    }
}

/// One entry of the conversation: a core timeline item or a review (exactly one is set).
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub item: Option<TimelineItemView>,
    pub review: Option<ReviewView>,
}

/// T0's items and the reviews, ordered by time (reviews after items of the same second).
pub fn interleave(items: Vec<TimelineItemView>, reviews: Vec<(i64, ReviewView)>) -> Vec<ConversationEntry> {
    let mut keyed: Vec<(i64, u8, usize, ConversationEntry)> = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| (item.cursor.at.seconds(), 0, index, ConversationEntry { item: Some(item), review: None }))
        .collect();
    keyed.extend(
        reviews.into_iter().enumerate().map(|(index, (at, review))| (at, 1, index, ConversationEntry { item: None, review: Some(review) })),
    );
    keyed.sort_by_key(|(at, rank, index, _)| (*at, *rank, *index));
    keyed.into_iter().map(|(_, _, _, entry)| entry).collect()
}

/// A reviewer in the sidebar with their latest state.
#[derive(Debug, Clone)]
pub struct ReviewerView {
    pub actor: ActorView,
    pub name: String,
    /// "Approved", "Changes requested", "Commented", "Requested", "Dismissed".
    pub state: &'static str,
    pub tone: &'static str,
    pub requested: bool,
}

/// The reviewers block of the sidebar.
#[derive(Debug, Clone)]
pub struct ReviewersView {
    pub reviewers: Vec<ReviewerView>,
    pub can_request: bool,
    pub request_url: String,
    pub remove_url: String,
}

impl ReviewersView {
    pub fn new(data: &ReviewData, can_request: bool, urls: &PullUrls) -> Self {
        let mut reviewers: Vec<ReviewerView> = data
            .latest
            .iter()
            .map(|(account, state)| {
                let (state, tone) = review_state_label(*state);
                ReviewerView { actor: ActorView::new(Some(account)), name: account.name.to_string(), state, tone, requested: false }
            })
            .collect();
        for account in &data.requested {
            match reviewers.iter_mut().find(|reviewer| reviewer.name == account.name.as_str()) {
                Some(reviewer) => reviewer.requested = true,
                None => reviewers.push(ReviewerView {
                    actor: ActorView::new(Some(account)),
                    name: account.name.to_string(),
                    state: "Awaiting review",
                    tone: "text-warning",
                    requested: true,
                }),
            }
        }
        Self { reviewers, can_request, request_url: urls.reviewers.clone(), remove_url: urls.remove_reviewer.clone() }
    }
}

fn review_state_label(state: ReviewState) -> (&'static str, &'static str) {
    match state {
        ReviewState::Approved => ("Approved", "text-success"),
        ReviewState::ChangesRequested => ("Changes requested", "text-danger"),
        ReviewState::Dismissed => ("Dismissed", "text-ink-faint"),
        ReviewState::Commented => ("Commented", "text-ink-muted"),
        ReviewState::Pending => ("Pending", "text-warning"),
    }
}

/// The submit-review form (conversation and Files changed).
#[derive(Debug, Clone)]
pub struct ReviewFormView {
    pub action_url: String,
    pub discard_url: String,
    pub pending_comments: usize,
    /// Approve and Request changes are offered (not to the author).
    pub can_decide: bool,
}

// ------------------------------------------------------------------ merge box

/// One requirement line.
#[derive(Debug, Clone)]
pub struct RequirementView {
    pub text: String,
    pub met: bool,
    /// `text-success`, `text-danger` or `text-warning` (checking).
    pub tone: &'static str,
    pub icon: &'static str,
}

/// The merge box at the end of the conversation.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct MergeBoxView {
    pub anchor: &'static str,
    pub requirements: Vec<RequirementView>,
    pub conflict_paths: Vec<String>,
    /// The head commit's checks, required ones first (milestone 3, S).
    pub checks: Option<crate::views::statuses::ChecksView>,
    pub checking: bool,
    pub can_merge: bool,
    pub ready: bool,
    pub needs_bypass: bool,
    pub can_delete_head: bool,
    pub action_url: String,
    pub expected_head: String,
    pub default_title: String,
    pub default_message: String,
    pub default_squash_title: String,
    pub default_squash_message: String,
    pub strategies: Vec<(&'static str, &'static str)>,
    pub draft_url: String,
    pub can_mark_ready: bool,
}

impl MergeBoxView {
    pub fn new(merge_box: &MergeBox, ctx: &PullContext, urls: &PullUrls, viewer: Option<&User>) -> Self {
        let requirements = merge_box
            .evaluation
            .checks
            .iter()
            .map(|check| {
                use rbit_core::pull::{ChecksStatus, Requirement};
                let checking = match check.requirement {
                    Requirement::NoConflicts(Mergeability::Unknown) => true,
                    // Milestone 3 (S): required checks still running are pending, not failed.
                    Requirement::RequiredChecks(checks) => checks.failing == 0,
                    _ => false,
                };
                let (tone, icon) = match (check.requirement, check.met, checking) {
                    // Statuses nobody requires: always "met", shown by their state.
                    (Requirement::Checks(ChecksStatus::Failing), ..) => ("text-danger", "x"),
                    (Requirement::Checks(ChecksStatus::NotConfigured), ..) => ("text-ink-faint", "circle-dot"),
                    (Requirement::Checks(ChecksStatus::Pending), ..) | (Requirement::RequiredChecks(_), false, true) => {
                        ("text-warning", "circle-dot")
                    }
                    (_, true, _) => ("text-success", "check"),
                    (_, false, true) => ("text-warning", "refresh"),
                    (_, false, false) => ("text-danger", "x"),
                };
                let met = check.met && !matches!(check.requirement, Requirement::Checks(ChecksStatus::Failing | ChecksStatus::Pending));
                RequirementView { text: check.requirement.describe(check.met), met, tone, icon }
            })
            .collect();
        let ready = merge_box.evaluation.can_merge();
        // Milestone 3 (S): when only running required checks are missing, the box is
        // "pending", not failed.
        let waiting_for_checks = !ready
            && merge_box
                .evaluation
                .checks
                .iter()
                .filter(|check| !check.met)
                .all(|check| matches!(check.requirement, rbit_core::pull::Requirement::RequiredChecks(checks) if checks.failing == 0));
        let is_author = viewer.is_some_and(|viewer| ctx.issue().is_author(viewer));
        Self {
            anchor: "merge-box",
            requirements,
            conflict_paths: merge_box.conflict_paths.clone(),
            checks: crate::views::statuses::ChecksView::new(&merge_box.statuses, &merge_box.rules.required_contexts),
            checking: merge_box.mergeable == Mergeability::Unknown || waiting_for_checks,
            can_merge: merge_box.can_merge,
            ready,
            needs_bypass: !ready && merge_box.evaluation.can_merge_with_bypass(),
            can_delete_head: merge_box.can_delete_head,
            action_url: urls.merge.clone(),
            expected_head: ctx.pull.head_sha.clone(),
            default_title: merge_box.default_title.clone(),
            default_message: merge_box.default_message.clone(),
            default_squash_title: merge_box.default_squash_title.clone(),
            default_squash_message: merge_box.default_squash_message.clone(),
            strategies: [MergeStrategy::Merge, MergeStrategy::Squash, MergeStrategy::Rebase]
                .into_iter()
                .map(|strategy| {
                    let label = match strategy {
                        MergeStrategy::Merge => "Create a merge commit",
                        MergeStrategy::Squash => "Squash and merge",
                        MergeStrategy::Rebase => "Rebase and merge",
                    };
                    (strategy.as_str(), label)
                })
                .collect(),
            draft_url: urls.draft.clone(),
            can_mark_ready: ctx.pull.is_draft && (is_author || ctx.repo().access.can_write()),
        }
    }
}

/// After merging or closing: the head branch can be deleted.
#[derive(Debug, Clone)]
pub struct ClosedBoxView {
    pub merged: bool,
    pub merge_commit: Option<(String, String)>,
    pub can_delete_branch: bool,
    pub delete_url: String,
    pub head_label: String,
}

// ------------------------------------------------------------------ pages

#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/show.html")]
pub struct PullPage {
    pub page: Page,
    pub header: RepoHeader,
    pub pull: PullHeaderView,
    pub thread: ThreadView,
    pub entries: Vec<ConversationEntry>,
    pub merge_box: Option<MergeBoxView>,
    pub closed_box: Option<ClosedBoxView>,
    pub review_form: Option<ReviewFormView>,
    pub reviewers: ReviewersView,
    pub sidebar: IssueSidebar,
    /// "Convert to draft" for open, ready pull requests (author or writers).
    pub can_convert_to_draft: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/commits.html")]
pub struct PullCommitsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub pull: PullHeaderView,
    pub commits: Vec<CommitView>,
    pub more_commits: bool,
}

/// One fixed-height row of the Files changed tab: a line (with its anchor and comment
/// link) or a hunk header or end-of-file note (`line.is_line()` false, empty anchor).
#[derive(Debug, Clone)]
pub struct DiffRowView {
    pub line: DiffLineView,
    /// `{file_anchor}-L{n}` or `-R{n}` ([`line_anchor`]); empty when not a line.
    pub anchor: String,
    /// The line number on the row's side, as in "Comment on line N".
    pub number: u32,
    /// `?comment=<anchor>#<anchor>`: opens the inline form (no JavaScript).
    pub comment_url: String,
}

/// The comments under one line and, when requested, the form for a new one. Not a
/// fixed-height row, so it ends the block before it and renders between blocks.
#[derive(Debug, Clone)]
pub struct DiffThreadView {
    /// The line's anchor.
    pub anchor: String,
    /// `left` or `right`.
    pub side: &'static str,
    pub line: u32,
    pub comments: Vec<ReviewCommentView>,
    pub show_form: bool,
}

/// A run of rows or a thread, in page order.
#[derive(Debug, Clone)]
pub enum DiffPart {
    Rows(Block<DiffRowView>),
    Thread(DiffThreadView),
}

/// A file's text diff on the Files changed tab.
#[derive(Debug, Clone)]
pub struct PullDiffText {
    pub parts: Vec<DiffPart>,
    /// See [`width_classes`].
    pub width_classes: String,
}

/// What a changed file shows.
#[derive(Debug, Clone)]
pub enum PullDiffBody {
    Text(PullDiffText),
    Binary,
    TooLarge,
    NoContentChanges,
    Submodule { old: String, new: String },
}

#[derive(Debug, Clone)]
pub struct PullFileView {
    pub anchor: String,
    pub path: String,
    pub old_path: Option<String>,
    pub status: &'static str,
    pub additions: usize,
    pub deletions: usize,
    pub stats_known: bool,
    pub view_url: Option<String>,
    pub body: PullDiffBody,
}

impl PullFileView {
    /// The file views of a pull request's diff with the visible, current comments placed
    /// under their lines; `form_at` opens the inline form at that anchor.
    pub fn list(
        files: &[FileDiff],
        head: &str,
        repo_urls: &RepoUrls,
        urls: &PullUrls,
        comments: &[ReviewCommentView],
        form_at: Option<&str>,
        can_comment: bool,
    ) -> Vec<Self> {
        let mut by_anchor: HashMap<String, Vec<ReviewCommentView>> = HashMap::new();
        for comment in comments.iter().filter(|comment| !comment.outdated) {
            let anchor = comment.line_url.rsplit_once('#').map_or_else(String::new, |(_, anchor)| anchor.to_owned());
            by_anchor.entry(anchor).or_default().push(comment.clone());
        }
        files
            .iter()
            .map(|file| {
                let status = match file.status {
                    ChangeStatus::Added => "Added",
                    ChangeStatus::Deleted => "Deleted",
                    ChangeStatus::Modified => "Modified",
                    ChangeStatus::Renamed => "Renamed",
                    ChangeStatus::Copied => "Copied",
                };
                let body = match &file.body {
                    FileDiffBody::Text(hunks) => {
                        PullDiffBody::Text(diff_text(hunks, &file.path, urls, &mut by_anchor, form_at, can_comment))
                    }
                    FileDiffBody::Binary => PullDiffBody::Binary,
                    FileDiffBody::TooLarge => PullDiffBody::TooLarge,
                    FileDiffBody::ModeOnly => PullDiffBody::NoContentChanges,
                    FileDiffBody::Submodule { old, new } => {
                        let short =
                            |id: &Option<String>| id.as_deref().map_or_else(|| "none".to_owned(), |id| id.chars().take(7).collect());
                        PullDiffBody::Submodule { old: short(old), new: short(new) }
                    }
                };
                Self {
                    anchor: file_anchor(&file.path),
                    path: file.path.clone(),
                    old_path: file.old_path.clone(),
                    status,
                    additions: file.additions,
                    deletions: file.deletions,
                    stats_known: file.stats_known,
                    view_url: (file.status != ChangeStatus::Deleted).then(|| repo_urls.blob(head, &file.path)),
                    body,
                }
            })
            .collect()
    }
}

/// The rows of `hunks` in blocks, each line's comments (and the inline form at `form_at`)
/// after it in a thread that ends the block before it.
fn diff_text(
    hunks: &[Hunk],
    path: &str,
    urls: &PullUrls,
    comments: &mut HashMap<String, Vec<ReviewCommentView>>,
    form_at: Option<&str>,
    can_comment: bool,
) -> PullDiffText {
    let plain = |line: DiffLineView| DiffRowView { line, anchor: String::new(), number: 0, comment_url: String::new() };
    let mut parts = Vec::new();
    let mut rows = Blocker::new();
    for hunk in hunks {
        rows.push(plain(DiffLineView::hunk(&hunk.header)));
        for line in &hunk.lines {
            let (side, number_on_side) = match line.kind {
                LineKind::Removed => (ReviewSide::Left, line.old_number),
                LineKind::Added | LineKind::Context => (ReviewSide::Right, line.new_number),
            };
            let number = number_on_side.and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
            let anchor = line_anchor(path, side, number);
            let mut threads = comments.remove(&anchor).unwrap_or_default();
            // Comments on the left side of a context line show under it too.
            if line.kind == LineKind::Context
                && let Some(old) = line.old_number.and_then(|n| u32::try_from(n).ok())
            {
                threads.extend(comments.remove(&line_anchor(path, ReviewSide::Left, old)).unwrap_or_default());
            }
            let show_form = can_comment && form_at == Some(anchor.as_str());
            rows.push(DiffRowView {
                line: DiffLineView::line(line),
                comment_url: format!("{}?comment={anchor}#{anchor}", urls.files),
                anchor: anchor.clone(),
                number,
            });
            if line.missing_newline {
                rows.push(plain(DiffLineView::eof()));
            }
            if !threads.is_empty() || show_form {
                parts.extend(rows.take().into_iter().map(DiffPart::Rows));
                parts.push(DiffPart::Thread(DiffThreadView {
                    anchor,
                    side: if side == ReviewSide::Left { "left" } else { "right" },
                    line: number,
                    comments: threads,
                    show_form,
                }));
            }
        }
    }
    parts.extend(rows.finish().into_iter().map(DiffPart::Rows));
    PullDiffText { parts, width_classes: width_classes(hunks) }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/files.html")]
pub struct PullFilesPage {
    pub page: Page,
    pub header: RepoHeader,
    pub pull: PullHeaderView,
    pub files: Vec<PullFileView>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
    pub can_comment: bool,
    pub signed_in: bool,
    pub sign_in_url: String,
    pub review_form: Option<ReviewFormView>,
    /// Outdated comments are listed in the conversation only.
    pub outdated_count: usize,
}

// ------------------------------------------------------------------ list

/// One label of a list row (colour as an SVG fill).
#[derive(Debug, Clone)]
pub struct LabelView {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct PullRowView {
    pub url: String,
    pub title: String,
    pub number: String,
    pub badge: StateBadge,
    pub author: String,
    pub when: String,
    pub when_iso: String,
    pub labels: Vec<LabelView>,
    pub head_label: String,
    pub base_branch: String,
    pub review: Option<(&'static str, &'static str)>,
    pub comment_count: u32,
}

impl PullRowView {
    pub fn new(item: &PullListItem, owner: &str, repo: &str, labels: Vec<LabelView>) -> Self {
        let issue = &item.issue;
        let review = if item.changes_requested > 0 {
            Some(("Changes requested", "text-danger"))
        } else if item.approvals > 0 {
            Some(("Approved", "text-success"))
        } else {
            None
        };
        let at = if issue.state == IssueState::Open { issue.created_at } else { issue.closed_at.unwrap_or(issue.updated_at) };
        Self {
            url: format!("/{owner}/{repo}/pull/{}", issue.number.get()),
            title: issue.title.to_string(),
            number: issue.number.to_string(),
            badge: badge(issue, item.pull.is_draft, item.pull.is_merged()),
            author: item.author.as_ref().map_or_else(|| "ghost".to_owned(), |author| author.name.to_string()),
            when: at.relative(),
            when_iso: at.iso8601(),
            labels,
            head_label: item.pull.head_branch.clone(),
            base_branch: item.pull.base_branch.clone(),
            review,
            comment_count: issue.comment_count,
        }
    }

    /// "opened" / "merged" / "closed", for the row's metadata line.
    pub fn verb(&self) -> &'static str {
        match self.badge.label {
            "Merged" => "merged",
            "Closed" => "closed",
            _ => "opened",
        }
    }
}

/// The Open / Closed tabs of the list.
#[derive(Debug, Clone)]
pub struct ListTabs {
    pub open_url: String,
    pub closed_url: String,
    pub open: u64,
    pub closed: u64,
    /// `open`, `closed` or `merged`.
    pub state: &'static str,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/list.html")]
pub struct PullListPage {
    pub page: Page,
    pub header: RepoHeader,
    pub tabs: ListTabs,
    pub rows: Vec<PullRowView>,
    pub pagination: Pagination,
    pub new_url: String,
    pub author_filter: Option<String>,
}

/// The "Open a pull request" page: pickers, the comparison and the form.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/new.html")]
pub struct NewPullPage {
    pub page: Page,
    pub header: RepoHeader,
    pub picker: ComparePicker,
    pub action_url: String,
    pub base: String,
    /// The head spec as submitted (`branch`, `owner:branch`, `owner/name:branch`).
    pub head: String,
    pub title: String,
    pub body: String,
    /// Why no pull request can be opened (no commits, unrelated histories, …).
    pub problem: Option<String>,
    pub can_create: bool,
    pub signed_in: bool,
    pub sign_in_url: String,
    pub commits: Vec<CommitView>,
    pub files: Vec<crate::views::commit::FileDiffView>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
}

/// `/{owner}/{repo}/pulls/new?base=…&head=…`, query-encoded.
pub fn new_pull_url(owner: &str, repo: &str, base: &str, head: &str) -> String {
    let query = |value: &str| percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string();
    format!("/{owner}/{repo}/pulls/new?base={}&head={}", query(base), query(head))
}

/// The page under `/pull/{n}/files` for a line, for redirects after commenting.
pub fn files_at(urls: &PullUrls, anchor: &str) -> String {
    format!("{}#{}", urls.files, encode_path(anchor))
}

#[cfg(test)]
mod tests {
    use rbit_core::Timestamp;
    use rbit_services::timeline::{TimelineCursor, TimelineItemId};

    use super::*;

    #[test]
    fn anchors_are_stable_and_short() {
        let anchor = file_anchor("src/main.rs");
        assert_eq!(anchor.len(), "diff-".len() + 8);
        assert_eq!(anchor, file_anchor("src/main.rs"));
        assert_ne!(anchor, file_anchor("src/lib.rs"));
        assert_eq!(line_anchor("src/main.rs", ReviewSide::Left, 3), format!("{anchor}-L3"));
        assert_eq!(review_anchor(7), "pullrequestreview-7");
        assert_eq!(new_pull_url("a", "b", "main", "bob:x/y"), "/a/b/pulls/new?base=main&head=bob%3Ax%2Fy");
    }

    #[test]
    fn reviews_interleave_with_timeline_items_by_time() {
        let item = |at: i64, id: i64| TimelineItemView {
            cursor: TimelineCursor { at: Timestamp(at), item: TimelineItemId::Comment(rbit_core::CommentId(id)) },
            comment: None,
            event: None,
        };
        let review = |id: i64| ReviewView {
            anchor: review_anchor(id),
            id,
            reviewer: ActorView::ghost(),
            sentence: "reviewed",
            icon: "eye",
            tone: "",
            body_html: String::new(),
            when: String::new(),
            when_iso: String::new(),
            comments: Vec::new(),
            dismissal: None,
            can_dismiss: false,
            dismiss_url: String::new(),
        };
        let entries = interleave(vec![item(10, 1), item(30, 2)], vec![(20, review(1)), (30, review(2))]);
        let order: Vec<String> = entries
            .iter()
            .map(|entry| {
                entry
                    .review
                    .as_ref()
                    .map_or_else(|| format!("item{}", entry.item.as_ref().unwrap().cursor.at.seconds()), |r| r.anchor.clone())
            })
            .collect();
        assert_eq!(order, ["item10", "pullrequestreview-1", "item30", "pullrequestreview-2"]);
    }

    fn page() -> Page {
        Page {
            site_name: "rbit".to_owned(),
            title: None,
            viewer: None,
            path: "/alice/demo/pulls".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    fn commits_page(pull: PullHeaderView) -> String {
        PullCommitsPage { page: page(), header: RepoHeader::default(), pull, commits: Vec::new(), more_commits: false }.render().unwrap()
    }

    #[test]
    fn deferred_counts_render_empty_slots_and_one_trigger() {
        let mut view = header_view();
        view.counts = PullCountsView { commits: Some(2), files: None };
        view.counts_url = Some("/alice/demo/pull/3/counts/a...b".to_owned());
        let html = commits_page(view);
        assert_eq!(html.matches(r#"hx-get="/alice/demo/pull/3/counts/a...b""#).count(), 1, "{html}");
        for slot in [CountSlot::CommitsPhrase, CountSlot::Commits, CountSlot::Files] {
            assert_eq!(html.matches(&format!(r#"id="{}""#, count_slot(slot))).count(), 1, "{slot:?}");
        }
        assert!(html.contains(">2 commits</span>"), "{html}");
        assert!(html.contains(">\u{2013}</span>"), "the unknown file count is a dash: {html}");
        assert!(!html.contains("hx-swap-oob"));

        let known = commits_page(header_view());
        assert!(!known.contains("/counts/"), "no request when the counts are known");
        assert!(known.contains(">1 commit</span>"));
    }

    #[test]
    fn the_counts_fragment_is_only_out_of_band_slots() {
        let html = PullCountsFragment { counts: PullCountsView::known(1, 12) }.render().unwrap();
        assert_eq!(html.matches("<span").count(), 3, "{html}");
        assert_eq!(html.matches(r#"hx-swap-oob="true""#).count(), 3, "{html}");
        for (slot, text) in [(CountSlot::CommitsPhrase, ">1 commit<"), (CountSlot::Commits, ">1<"), (CountSlot::Files, ">12<")] {
            assert!(html.contains(&format!(r#"id="{}""#, count_slot(slot))), "{slot:?}");
            assert!(html.contains(text), "{text}: {html}");
        }
    }

    fn header_view() -> PullHeaderView {
        PullHeaderView {
            title: "Fix <the> crash".to_owned(),
            number: "#3".to_owned(),
            badge: StateBadge::draft(),
            author: ActorView::ghost(),
            verb: "wants to merge",
            base_branch: "main".to_owned(),
            base_url: "/alice/demo/tree/main".to_owned(),
            head_label: "bob:fix".to_owned(),
            head_url: None,
            urls: PullUrls::new("alice", "demo", 3),
            active: "files",
            counts: PullCountsView::known(1, 1),
            counts_url: None,
            is_draft: true,
        }
    }

    #[test]
    fn the_list_renders_rows_with_label_dots_and_no_inline_styles() {
        let row = PullRowView {
            url: "/alice/demo/pull/3".to_owned(),
            title: "Fix <the> crash".to_owned(),
            number: "#3".to_owned(),
            badge: StateBadge::draft(),
            author: "bob".to_owned(),
            when: "now".to_owned(),
            when_iso: String::new(),
            labels: vec![LabelView { name: "bug".to_owned(), color: "#d73a4a".to_owned() }],
            head_label: "fix".to_owned(),
            base_branch: "main".to_owned(),
            review: Some(("Approved", "text-success")),
            comment_count: 2,
        };
        let html = PullListPage {
            page: page(),
            header: RepoHeader::default(),
            tabs: ListTabs {
                open_url: "?state=open".to_owned(),
                closed_url: "?state=closed".to_owned(),
                open: 1,
                closed: 4,
                state: "open",
            },
            rows: vec![row],
            pagination: Pagination { page: 1, page_count: 1, previous_url: None, next_url: None },
            new_url: "/alice/demo/compare".to_owned(),
            author_filter: None,
        }
        .render()
        .unwrap();
        for needle in ["Fix &#60;the&#62; crash", "fill=\"#d73a4a\"", "4</span> Closed", "Approved", ">Draft<", "opened"] {
            assert!(html.contains(needle), "{needle} missing");
        }
        assert!(!html.contains("style="), "no inline styles (strict CSP)");
    }

    fn comment_on(urls: &PullUrls, anchor: &str, id: i64) -> ReviewCommentView {
        ReviewCommentView {
            anchor: review_comment_anchor(id),
            author: ActorView::ghost(),
            body_html: String::new(),
            when: String::new(),
            when_iso: String::new(),
            path: "f".to_owned(),
            line_label: String::new(),
            hunk_header: String::new(),
            hunk: Vec::new(),
            outdated: false,
            pending: false,
            is_reply: false,
            line_url: format!("{}#{anchor}", urls.files),
            reply_to: id,
        }
    }

    /// One string per part: `rows:<kinds>` or `thread:<anchor>`.
    fn outline(text: &PullDiffText) -> Vec<String> {
        text.parts
            .iter()
            .map(|part| match part {
                DiffPart::Rows(block) => format!("rows:{}", block.rows().iter().map(|row| row.line.kind).collect::<Vec<_>>().join(",")),
                DiffPart::Thread(thread) => format!("thread:{}{}", thread.anchor, if thread.show_form { "+form" } else { "" }),
            })
            .collect()
    }

    #[test]
    fn threads_and_the_form_end_blocks_and_every_row_stays_in_order() {
        let changes = rbit_git::diff::diff_bytes(b"a\nb\nc", b"a\nB\nc");
        let urls = PullUrls::new("alice", "demo", 3);
        let left = line_anchor("f", ReviewSide::Left, 2);
        let context = line_anchor("f", ReviewSide::Right, 1);
        let mut comments = HashMap::new();
        comments.insert(left.clone(), vec![comment_on(&urls, &left, 1)]);
        comments.insert(context.clone(), vec![comment_on(&urls, &context, 2)]);
        let form_at = line_anchor("f", ReviewSide::Right, 3);
        let text = diff_text(&changes.hunks, "f", &urls, &mut comments, Some(&form_at), true);
        assert_eq!(
            outline(&text),
            [
                "rows:hunk,ctx".to_owned(),
                format!("thread:{context}"),
                "rows:del".to_owned(),
                format!("thread:{left}"),
                "rows:add,ctx,eof".to_owned(),
                format!("thread:{form_at}+form"),
            ]
        );
        assert!(comments.is_empty(), "every comment is placed");
        let rows: Vec<&DiffRowView> = text
            .parts
            .iter()
            .filter_map(|part| if let DiffPart::Rows(block) = part { Some(block.rows()) } else { None })
            .flatten()
            .collect();
        assert_eq!(
            rows.iter().filter(|row| row.line.is_line()).map(|row| row.line.text.as_str()).collect::<Vec<_>>(),
            ["a", "b", "B", "c"]
        );
        assert_eq!(rows[2].anchor, left);
        assert_eq!(rows[2].number, 2);
        assert!(rows[0].anchor.is_empty() && rows[5].anchor.is_empty(), "hunk headers and eof notes have no anchor");
        assert_eq!(rows[3].comment_url, format!("{}?comment={}#{}", urls.files, rows[3].anchor, rows[3].anchor));
        // Without the right to comment, the form never opens.
        let text = diff_text(&changes.hunks, "f", &urls, &mut HashMap::new(), Some(&form_at), false);
        assert_eq!(outline(&text), ["rows:hunk,ctx,del,add,ctx,eof"]);
    }

    #[test]
    fn long_files_fill_whole_blocks_between_threads() {
        let old = crate::views::commit::tests::numbered("", 400);
        let new = crate::views::commit::tests::numbered("y", 400);
        let changes = rbit_git::diff::diff_bytes(old.as_bytes(), new.as_bytes());
        let urls = PullUrls::new("alice", "demo", 3);
        let anchor = line_anchor("f", ReviewSide::Right, 300);
        let mut comments = HashMap::from([(anchor.clone(), vec![comment_on(&urls, &anchor, 1)])]);
        let text = diff_text(&changes.hunks, "f", &urls, &mut comments, None, true);
        let mut sizes = Vec::new();
        for part in &text.parts {
            match part {
                DiffPart::Rows(block) => sizes.push(block.len().to_string()),
                DiffPart::Thread(_) => sizes.push("thread".to_owned()),
            }
        }
        // hunk + 400 deletions + 300 additions, then the thread, then 100 additions.
        assert_eq!(sizes, ["128", "128", "128", "128", "128", "61", "thread", "100"]);
        assert_eq!(text.width_classes, "diff-w3 diff-c24");
    }

    #[test]
    fn files_render_line_anchors_comments_and_one_inline_form() {
        let changes = rbit_git::diff::diff_bytes(b"a\nb\n", b"a\nB\n");
        let file = FileDiff {
            path: "src/lib.rs".to_owned(),
            old_path: None,
            status: ChangeStatus::Modified,
            additions: changes.additions,
            deletions: changes.deletions,
            stats_known: true,
            body: FileDiffBody::Text(changes.hunks),
        };
        let urls = PullUrls::new("alice", "demo", 3);
        let anchor = line_anchor("src/lib.rs", ReviewSide::Right, 2);
        let comment = ReviewCommentView {
            anchor: review_comment_anchor(9),
            author: ActorView::ghost(),
            body_html: "<p>Why?</p>".to_owned(),
            when: "now".to_owned(),
            when_iso: String::new(),
            path: "src/lib.rs".to_owned(),
            line_label: "R2".to_owned(),
            hunk_header: String::new(),
            hunk: Vec::new(),
            outdated: false,
            pending: true,
            is_reply: false,
            line_url: format!("{}#{anchor}", urls.files),
            reply_to: 9,
        };
        let files = PullFileView::list(&[file], &"a".repeat(40), &RepoUrls::new("alice", "demo"), &urls, &[comment], Some(&anchor), true);
        let html = PullFilesPage {
            page: page(),
            header: RepoHeader::default(),
            pull: header_view(),
            files,
            additions: 1,
            deletions: 1,
            truncated: false,
            can_comment: true,
            signed_in: true,
            sign_in_url: String::new(),
            review_form: Some(ReviewFormView {
                action_url: urls.reviews.clone(),
                discard_url: urls.discard_review.clone(),
                pending_comments: 1,
                can_decide: true,
            }),
            outdated_count: 0,
        }
        .render()
        .unwrap();
        assert!(html.contains(&format!("id=\"{anchor}\"")));
        assert!(html.contains(&format!("?comment={anchor}#{anchor}")));
        let link = format!(
            r##"<a href="/alice/demo/pull/3/files?comment={anchor}#{anchor}" class="num" hx-get="/alice/demo/pull/3/files?comment={anchor}#{anchor}" hx-select="#{file}" hx-target="#{file}" hx-swap="outerHTML""##,
            file = file_anchor("src/lib.rs"),
        );
        assert!(html.contains(&format!(r#"<div id="{anchor}" class="diff-add">{link} aria-label="Comment on line 2">"#)), "{html}");
        assert!(html.contains(&format!(r#"{link} tabindex="-1">2</a>B</div>"#)));
        // The thread and the form end the block: they sit between blocks, outside any.
        assert!(html.contains("</div>\n      <div class=\"diff-comment\">"));
        assert!(!html.contains("<table"));
        assert_eq!(html.matches("Add single comment").count(), 1, "the form opens only at the requested line");
        for needle in ["id=\"r9\"", "<p>Why?</p>", "Pending", "Finish your review", "name=\"in_reply_to\" value=\"9\"", "Request changes"] {
            assert!(html.contains(needle), "{needle} missing");
        }
        assert!(!html.contains("style="));
    }
}
