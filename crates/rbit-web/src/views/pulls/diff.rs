//! Pull request file diff view models.

use super::{
    BLOCKED_ABOVE_ROWS, Block, Blocker, ChangeStatus, DiffLineView, FILE_HEADER_ROWS, FileDiff, FileDiffBody, HashMap, Hunk, LineKind,
    PLACEHOLDER_ROWS, Page, PullHeaderView, RepoHeader, RepoUrls, ReviewCommentView, ReviewFormView, ReviewSide, Template, WebTemplate,
    file_anchor, line_anchor_in, width_classes,
};

/// One fixed-height row of the Files changed tab: a line (with its anchor) or a hunk
/// header or end-of-file note (`line.is_line()` false).
///
/// A line row is the same markup as a read-only diff's: the file says where comments go
/// (`data-comment`) and diff.js opens the form from a click or key on a number, so a long
/// diff carries no link per line. Without scripts, a hunk header links to the form at its
/// first line: for a hunk header of a viewer who may comment, `anchor` and `number` name
/// that line.
#[derive(Debug, Clone)]
pub struct DiffRowView {
    pub line: DiffLineView,
    /// `{file_anchor}-L{n}` or `-R{n}` ([`super::line_anchor`]): the line's, or the first
    /// line's of a hunk whose header links to the form; otherwise empty.
    pub anchor: String,
    /// The line number on the row's side, as in "Comment on line N"; 0 without an anchor.
    pub number: u32,
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
    /// The number of rows, threads not counted.
    pub rows: usize,
    /// Whether the parts include a thread: comments and forms are not fixed-height rows,
    /// so such a file has no exact height and is never contained as a whole.
    pub has_threads: bool,
    /// Whether each block of rows renders as its own `.vblock`: in diffs of more than
    /// [`BLOCKED_ABOVE_ROWS`] rows, and around threads, where blocks are the only
    /// containment with an exact height.
    pub blocked: bool,
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
    pub status_key: &'static str,
    pub additions: usize,
    pub deletions: usize,
    pub stats_known: bool,
    pub view_url: Option<String>,
    pub body: PullDiffBody,
}

/// Rows the "View file" bar under a file's header takes (`.diff-file-bar`, app.css).
pub const FILE_BAR_ROWS: usize = 2;

impl PullFileView {
    /// Share the commit navigator while keeping stable review anchors and file indexes.
    pub fn tree(files: &[Self]) -> Vec<crate::views::commit::FileTreeRow> {
        use crate::views::commit::{FileTreeLeaf, file_tree};

        file_tree(files.iter().enumerate().map(|(index, file)| {
            (&*file.path, FileTreeLeaf { index, anchor: file.anchor.clone(), status: file.status, status_key: file.status_key })
        }))
    }

    /// The open file's exact height in rows, chrome included: `data-rows` on the file, its
    /// placeholder while off screen (see [`crate::views::blocks`]). None when comment
    /// threads or the inline form make its height unknown; its blocks are contained
    /// instead.
    #[must_use]
    pub fn rows(&self) -> Option<usize> {
        let body = match &self.body {
            PullDiffBody::Text(text) if text.has_threads => return None,
            PullDiffBody::Text(text) => text.rows,
            _ => PLACEHOLDER_ROWS,
        };
        Some(FILE_HEADER_ROWS + if self.view_url.is_some() { FILE_BAR_ROWS } else { 0 } + body)
    }

    /// The file views of a pull request's diff with the visible, current comments placed
    /// under their lines; `form_at` opens the inline form at that anchor. The diffs' paths
    /// and lines move into the views.
    pub fn list(
        files: Vec<FileDiff>,
        head: &str,
        repo_urls: &RepoUrls,
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
            .into_iter()
            .map(|file| {
                let (status, status_key) = match file.status {
                    ChangeStatus::Added => ("Added", "added"),
                    ChangeStatus::Deleted => ("Deleted", "deleted"),
                    ChangeStatus::Modified => ("Modified", "modified"),
                    ChangeStatus::Renamed => ("Renamed", "renamed"),
                    ChangeStatus::Copied => ("Copied", "copied"),
                };
                let body = match file.body {
                    FileDiffBody::Text(hunks) => PullDiffBody::Text(diff_text(hunks, &file.path, &mut by_anchor, form_at, can_comment)),
                    FileDiffBody::Binary => PullDiffBody::Binary,
                    FileDiffBody::TooLarge => PullDiffBody::TooLarge,
                    FileDiffBody::ModeOnly => PullDiffBody::NoContentChanges,
                    FileDiffBody::Submodule { old, new } => {
                        let short =
                            |id: &Option<String>| id.as_deref().map_or_else(|| "none".to_owned(), |id| id.chars().take(7).collect());
                        PullDiffBody::Submodule { old: short(&old), new: short(&new) }
                    }
                };
                Self {
                    anchor: file_anchor(&file.path),
                    view_url: (file.status != ChangeStatus::Deleted).then(|| repo_urls.blob(head, &file.path)),
                    path: file.path,
                    old_path: file.old_path,
                    status,
                    status_key,
                    additions: file.additions,
                    deletions: file.deletions,
                    stats_known: file.stats_known,
                    body,
                }
            })
            .collect()
    }
}

/// The rows of `hunks` in blocks, each line's comments (and the inline form at `form_at`)
/// after it in a thread that ends the block before it.
pub(super) fn diff_text(
    hunks: Vec<Hunk>,
    path: &str,
    comments: &mut HashMap<String, Vec<ReviewCommentView>>,
    form_at: Option<&str>,
    can_comment: bool,
) -> PullDiffText {
    let width_classes = width_classes(&hunks);
    // One digest per file: every line's anchor extends it.
    let file = file_anchor(path);
    let plain = |line: DiffLineView| DiffRowView { line, anchor: String::new(), number: 0 };
    let mut parts = Vec::new();
    let mut rows = Blocker::new();
    for hunk in hunks {
        let mut header = plain(DiffLineView::hunk(hunk.header));
        if can_comment && let Some(first) = hunk.lines.first() {
            let (side, number) = side_of(first);
            header.anchor = line_anchor_in(&file, side, number);
            header.number = number;
        }
        rows.push(header);
        for line in hunk.lines {
            let (side, number) = side_of(&line);
            let anchor = line_anchor_in(&file, side, number);
            let mut threads = Vec::new();
            if !comments.is_empty() {
                threads = comments.remove(&anchor).unwrap_or_default();
                // Comments on the left side of a context line show under it too.
                if line.kind == LineKind::Context
                    && let Some(old) = line.old_number.and_then(|n| u32::try_from(n).ok())
                {
                    threads.extend(comments.remove(&line_anchor_in(&file, ReviewSide::Left, old)).unwrap_or_default());
                }
            }
            let show_form = can_comment && form_at == Some(anchor.as_str());
            let missing_newline = line.missing_newline;
            let thread_anchor = (!threads.is_empty() || show_form).then(|| anchor.clone());
            rows.push(DiffRowView { line: DiffLineView::owned_line(line), anchor, number });
            if missing_newline {
                rows.push(plain(DiffLineView::eof()));
            }
            if let Some(anchor) = thread_anchor {
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
    let mut rows = 0;
    let mut has_threads = false;
    for part in &parts {
        match part {
            DiffPart::Rows(block) => rows += block.len(),
            DiffPart::Thread(_) => has_threads = true,
        }
    }
    PullDiffText { parts, rows, has_threads, blocked: has_threads || rows > BLOCKED_ABOVE_ROWS, width_classes }
}

/// The side a review comment on `line` is on, and its line number there.
fn side_of(line: &rbit_git::diff::DiffLine) -> (ReviewSide, u32) {
    let (side, number) = match line.kind {
        LineKind::Removed => (ReviewSide::Left, line.old_number),
        LineKind::Added | LineKind::Context => (ReviewSide::Right, line.new_number),
    };
    (side, number.and_then(|n| u32::try_from(n).ok()).unwrap_or(0))
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
