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

mod diff;
mod list;
mod merge;
mod reviews;

pub use diff::{DiffPart, DiffRowView, DiffThreadView, FILE_BAR_ROWS, PullDiffBody, PullDiffText, PullFileView, PullFilesPage};
pub use list::{LabelView, ListTabs, PullListPage, PullRowView};
pub use merge::{ClosedBoxView, MergeBoxView, RequirementView};
pub use reviews::{ConversationEntry, ReviewCommentView, ReviewFormView, ReviewView, ReviewerView, ReviewersView, interleave};

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
        blocks::{BLOCKED_ABOVE_ROWS, Block, Blocker},
        // `DiffBodyView` is matched by `repo/file_diffs.html` (the new pull request page).
        commit::{DiffBodyView, DiffLineView, FILE_HEADER_ROWS, PLACEHOLDER_ROWS, width_classes},
        compare::ComparePicker,
        components::Pagination,
        issues::IssueSidebar,
        layout::Page,
        repo::{CommitView, RepoHeader},
        timeline::{ActorView, StateBadge, ThreadView, TimelineItemView},
    },
};

#[cfg(test)]
use diff::diff_text;

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
            branding: rbit_core::config::BrandingConfig::default(),
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
    fn a_lone_carriage_return_stays_inside_its_row() {
        let changes = rbit_git::diff::diff_bytes(b"foo\rbar\n", b"foo\rbaz\n");
        let urls = PullUrls::new("alice", "demo", 3);
        let text = diff_text(&changes.hunks, "f", &urls, &mut HashMap::new(), None, true);
        let lines: Vec<&str> = text
            .parts
            .iter()
            .filter_map(|part| if let DiffPart::Rows(block) = part { Some(block.rows()) } else { None })
            .flatten()
            .filter(|row| row.line.is_line())
            .map(|row| row.line.text.as_str())
            .collect();
        assert_eq!(lines, ["foo\u{240D}bar", "foo\u{240D}baz"]);
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
        assert_eq!(text.width_classes, "diff-w3 cols-24");
        assert_eq!(text.rows, 801);
        assert!(text.has_threads && text.blocked, "a thread's height is unknown, so its file keeps exact blocks");
        // Without the thread, the same rows are blocked only for being more than BLOCKED_ABOVE_ROWS.
        let text = diff_text(&changes.hunks, "f", &urls, &mut HashMap::new(), None, true);
        assert!(!text.has_threads && text.blocked);
        let short = diff_text(&rbit_git::diff::diff_bytes(b"a\n", b"b\n").hunks, "f", &urls, &mut HashMap::new(), None, true);
        assert_eq!((short.rows, short.has_threads, short.blocked), (3, false, false));
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
        // Without threads a file's height is known: header, "View file" bar and rows.
        let plain =
            PullFileView::list(std::slice::from_ref(&file), &"a".repeat(40), &RepoUrls::new("alice", "demo"), &urls, &[], None, true);
        assert_eq!(plain[0].rows(), Some(FILE_HEADER_ROWS + FILE_BAR_ROWS + 4));
        let files = PullFileView::list(&[file], &"a".repeat(40), &RepoUrls::new("alice", "demo"), &urls, &[comment], Some(&anchor), true);
        assert_eq!(files[0].rows(), None, "a thread makes the height unknown");
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
        assert!(html.contains(&format!(r#"<details id="{}" class="diff-file box group" open>"#, file_anchor("src/lib.rs"))));
        assert!(html.contains(r#"<div class="vblock" data-rows="4">"#));
        assert!(!html.contains("<table"));
        assert_eq!(html.matches("Add single comment").count(), 1, "the form opens only at the requested line");
        for needle in ["id=\"r9\"", "<p>Why?</p>", "Pending", "Finish your review", "name=\"in_reply_to\" value=\"9\"", "Request changes"] {
            assert!(html.contains(needle), "{needle} missing");
        }
        assert!(!html.contains("style="));
    }
}
