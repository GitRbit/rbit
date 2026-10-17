//! The single-commit page: message, metadata and file diffs.

use std::collections::BTreeMap;

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::{
    ChangeStatus, CommitDetail, FileDiff, FileDiffBody,
    diff::{DiffLine, Hunk, LineKind},
};

use rbit_services::statuses::CombinedStatus;

use crate::{
    urls::RepoUrls,
    views::{
        blocks::{self, Block, blocks},
        layout::Page,
        repo::{CommitView, RepoHeader},
        statuses::{CHECKS_ANCHOR, ChecksView, StatusMark},
    },
};

#[derive(Debug, Clone)]
pub struct ParentLink {
    pub short_id: String,
    pub url: String,
}

/// One fixed-height row of a unified diff: a hunk header, a changed or context line, or
/// Git's "No newline at end of file" note. Every row is exactly one `--row-h` tall, so a
/// block of rows has an exact height (see [`crate::views::blocks`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLineView {
    /// CSS modifier: `hunk`, `add`, `del`, `ctx` or `eof`. The sign (`+`, `-`) is drawn by
    /// app.css from it, so it is never copied with the code.
    pub kind: &'static str,
    /// Empty for hunk headers and end-of-file notes, and on the side a line is not on.
    pub old_number: String,
    pub new_number: String,
    /// The line (anything in it that would break it shown as a symbol, see
    /// [`crate::render::row_char`]), the hunk header (`@@ -a,b +c,d @@`) or the
    /// end-of-file note.
    pub text: String,
}

/// What an end-of-file row says.
pub const NO_NEWLINE: &str = "No newline at end of file";

impl DiffLineView {
    /// Whether the row is a line of the file, with line numbers (not a hunk header or an
    /// end-of-file note).
    #[must_use]
    pub fn is_line(&self) -> bool {
        matches!(self.kind, "add" | "del" | "ctx")
    }

    /// The hunk header row.
    #[must_use]
    pub fn hunk(header: &str) -> Self {
        Self { kind: "hunk", old_number: String::new(), new_number: String::new(), text: header.to_owned() }
    }

    /// The "No newline at end of file" row shown after a line that lacks one.
    #[must_use]
    pub fn eof() -> Self {
        Self { kind: "eof", old_number: String::new(), new_number: String::new(), text: NO_NEWLINE.to_owned() }
    }

    /// The row of one line of a hunk.
    #[must_use]
    pub fn line(line: &DiffLine) -> Self {
        let number = |n: Option<usize>| n.map(|n| n.to_string()).unwrap_or_default();
        let kind = match line.kind {
            LineKind::Added => "add",
            LineKind::Removed => "del",
            LineKind::Context => "ctx",
        };
        let text = crate::render::row_text(&line.text).into_owned();
        Self { kind, old_number: number(line.old_number), new_number: number(line.new_number), text }
    }
}

/// Widest line number column a class exists for (`diff-w1` … `diff-w7` in app.css).
const NUMBER_CLASSES: [&str; 7] = ["diff-w1", "diff-w2", "diff-w3", "diff-w4", "diff-w5", "diff-w6", "diff-w7"];

/// The class that makes a diff's line number columns as wide as the largest line number
/// in `hunks` needs: one `ch` per digit (the CSP forbids computed `style=` widths).
#[must_use]
pub fn number_class(hunks: &[Hunk]) -> &'static str {
    let largest =
        hunks.iter().flat_map(|hunk| &hunk.lines).flat_map(|line| [line.old_number, line.new_number]).flatten().max().unwrap_or(0);
    let digits = largest.checked_ilog10().map_or(1, |log| log as usize + 1);
    NUMBER_CLASSES[digits.min(NUMBER_CLASSES.len()) - 1]
}

/// The class that makes a diff at least as wide as its widest line, hunk headers
/// included ([`blocks::column_class`]).
#[must_use]
pub fn column_class(hunks: &[Hunk]) -> String {
    let widest = hunks
        .iter()
        .flat_map(|hunk| std::iter::once(hunk.header.as_str()).chain(hunk.lines.iter().map(|line| line.text.as_str())))
        .map(blocks::display_columns)
        .max()
        .unwrap_or(0);
    blocks::column_class(widest)
}

/// The width classes of a diff of `hunks`: [`number_class`] and [`column_class`].
#[must_use]
pub fn width_classes(hunks: &[Hunk]) -> String {
    format!("{} {}", number_class(hunks), column_class(hunks))
}

/// A file's text diff: its rows (hunk headers, lines and end-of-file notes, in order) in
/// blocks of at most [`BLOCK_ROWS`](crate::views::blocks::BLOCK_ROWS).
#[derive(Debug, Clone)]
pub struct DiffTextView {
    pub blocks: Vec<Block<DiffLineView>>,
    /// See [`width_classes`].
    pub width_classes: String,
}

impl DiffTextView {
    #[must_use]
    pub fn new(hunks: &[Hunk]) -> Self {
        let rows = hunks.iter().flat_map(|hunk| {
            std::iter::once(DiffLineView::hunk(&hunk.header)).chain(
                hunk.lines
                    .iter()
                    .flat_map(|line| std::iter::once(DiffLineView::line(line)).chain(line.missing_newline.then(DiffLineView::eof))),
            )
        });
        Self { blocks: blocks(rows), width_classes: width_classes(hunks) }
    }
}

#[derive(Debug, Clone)]
pub enum DiffBodyView {
    Text(DiffTextView),
    Binary,
    TooLarge,
    NoContentChanges,
    /// Submodule pointer change, shown as abbreviated commit ids.
    Submodule {
        old: String,
        new: String,
    },
}

#[derive(Debug, Clone)]
pub struct FileDiffView {
    /// In-page anchor, e.g. `diff-3`.
    pub anchor: String,
    pub path: String,
    /// `path` up to and including its last `/` (empty at the root), shown muted.
    pub directory: String,
    /// The last component of `path`.
    pub name: String,
    pub old_path: Option<String>,
    pub status: &'static str,
    /// CSS modifier for the status mark: `added`, `deleted`, `modified`, `renamed` or `copied`.
    pub status_key: &'static str,
    pub additions: usize,
    pub deletions: usize,
    /// False when the file wasn't diffed; the counts are then hidden.
    pub stats_known: bool,
    /// View the file as of this commit (absent for deletions).
    pub view_url: Option<String>,
    pub body: DiffBodyView,
}

/// Blocks in a file's change bar.
const DIFFSTAT_BLOCKS: usize = 5;

impl FileDiffView {
    /// The change bar: `DIFFSTAT_BLOCKS` cells, each `add`, `del` or `none`. A change of
    /// fewer lines than blocks fills only that many; either side keeps at least one block.
    pub fn diffstat(&self) -> [&'static str; DIFFSTAT_BLOCKS] {
        let mut cells = ["none"; DIFFSTAT_BLOCKS];
        let total = self.additions + self.deletions;
        if !self.stats_known || total == 0 {
            return cells;
        }
        let filled = total.min(DIFFSTAT_BLOCKS);
        let mut added = (self.additions * filled + total / 2) / total;
        if self.additions > 0 && added == 0 {
            added = 1;
        }
        if self.deletions > 0 && added == filled {
            added = filled - 1;
        }
        for (i, cell) in cells.iter_mut().enumerate().take(filled) {
            *cell = if i < added { "add" } else { "del" };
        }
        cells
    }
}

impl FileDiffView {
    /// Views for `files`, linking each to its version at `commit_id`.
    pub fn list(files: &[FileDiff], commit_id: &str, urls: &RepoUrls) -> Vec<Self> {
        files.iter().enumerate().map(|(i, f)| Self::new(i, f, commit_id, urls)).collect()
    }

    fn new(index: usize, file: &FileDiff, commit_id: &str, urls: &RepoUrls) -> Self {
        let (status, status_key) = match file.status {
            ChangeStatus::Added => ("Added", "added"),
            ChangeStatus::Deleted => ("Deleted", "deleted"),
            ChangeStatus::Modified => ("Modified", "modified"),
            ChangeStatus::Renamed => ("Renamed", "renamed"),
            ChangeStatus::Copied => ("Copied", "copied"),
        };
        let split = file.path.rfind('/').map_or(0, |i| i + 1);
        let body = match &file.body {
            FileDiffBody::Text(hunks) => DiffBodyView::Text(DiffTextView::new(hunks)),
            FileDiffBody::Binary => DiffBodyView::Binary,
            FileDiffBody::TooLarge => DiffBodyView::TooLarge,
            FileDiffBody::ModeOnly => DiffBodyView::NoContentChanges,
            FileDiffBody::Submodule { old, new } => {
                let short = |id: &Option<String>| id.as_deref().map_or_else(|| "none".to_owned(), |id| id[..7.min(id.len())].to_owned());
                DiffBodyView::Submodule { old: short(old), new: short(new) }
            }
        };
        Self {
            anchor: format!("diff-{index}"),
            path: file.path.clone(),
            directory: file.path[..split].to_owned(),
            name: file.path[split..].to_owned(),
            old_path: file.old_path.clone(),
            status,
            status_key,
            additions: file.additions,
            deletions: file.deletions,
            stats_known: file.stats_known,
            view_url: (file.status != ChangeStatus::Deleted).then(|| urls.blob(commit_id, &file.path)),
            body,
        }
    }
}

/// One row of the changed-file tree beside a diff: a folder or a file, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeRow {
    /// Nesting level, 0 at the root.
    pub depth: usize,
    /// Display name; a folder holding only one folder is merged with it (`docs/plugins`).
    pub name: String,
    /// The folder's or file's full path, which scopes collapsing and filtering.
    pub path: String,
    /// The diff a file row links to; `None` for folders.
    pub file: Option<FileTreeLeaf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeLeaf {
    /// Position of the file's diff in the list the tree was built from.
    pub index: usize,
    pub anchor: String,
    pub status: &'static str,
    pub status_key: &'static str,
}

/// The changed files as a folder tree, flattened in display order: folders first, then
/// files, each sorted by name (case-insensitively). The diffs are listed in the same order,
/// so reading down the page walks down the tree.
pub fn file_tree(files: &[FileDiffView]) -> Vec<FileTreeRow> {
    #[derive(Default)]
    struct Folder<'a> {
        folders: BTreeMap<&'a str, Folder<'a>>,
        files: Vec<(usize, &'a FileDiffView)>,
    }

    fn sort_key(name: &str) -> (String, &str) {
        (name.to_lowercase(), name)
    }

    fn flatten(folder: &Folder<'_>, prefix: &str, depth: usize, rows: &mut Vec<FileTreeRow>) {
        let mut folders: Vec<_> = folder.folders.iter().collect();
        folders.sort_by_key(|(name, _)| sort_key(name));
        for (name, mut child) in folders {
            let mut name = (*name).to_owned();
            while child.files.is_empty()
                && child.folders.len() == 1
                && let Some((inner_name, inner)) = child.folders.iter().next()
            {
                name = format!("{name}/{inner_name}");
                child = inner;
            }
            let path = format!("{prefix}{name}");
            rows.push(FileTreeRow { depth, name, path: path.clone(), file: None });
            flatten(child, &format!("{path}/"), depth + 1, rows);
        }
        let mut files = folder.files.clone();
        files.sort_by(|(_, a), (_, b)| sort_key(&a.name).cmp(&sort_key(&b.name)));
        for (index, file) in files {
            rows.push(FileTreeRow {
                depth,
                name: file.name.clone(),
                path: file.path.clone(),
                file: Some(FileTreeLeaf { index, anchor: file.anchor.clone(), status: file.status, status_key: file.status_key }),
            });
        }
    }

    let mut root = Folder::default();
    for (index, file) in files.iter().enumerate() {
        let mut folder = &mut root;
        for part in file.directory.split('/').filter(|part| !part.is_empty()) {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push((index, file));
    }
    let mut rows = Vec::with_capacity(files.len() * 2);
    flatten(&root, "", 0, &mut rows);
    rows
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/commit.html")]
pub struct CommitPage {
    pub page: Page,
    pub header: RepoHeader,
    pub commit: CommitView,
    pub body: Option<String>,
    pub committer: Option<String>,
    pub parents: Vec<ParentLink>,
    pub files: Vec<FileDiffView>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
    /// The commit's checks (milestone 3, S); `None` without statuses.
    pub checks: Option<ChecksView>,
}

impl CommitPage {
    pub fn new(page: Page, header: RepoHeader, detail: &CommitDetail, urls: &RepoUrls) -> Self {
        let commit = &detail.commit;
        Self {
            page,
            header,
            commit: CommitView::new(commit, urls),
            body: commit.body.clone(),
            committer: (detail.committer_name != commit.author_name).then(|| detail.committer_name.clone()),
            parents: commit
                .parent_ids
                .iter()
                .map(|id| ParentLink { short_id: id[..7.min(id.len())].to_owned(), url: urls.commit(id) })
                .collect(),
            files: FileDiffView::list(&detail.files, &commit.id, urls),
            additions: detail.additions(),
            deletions: detail.deletions(),
            truncated: detail.truncated,
            checks: None,
        }
    }

    /// Adds the commit's combined status: the mark beside the title and the checks.
    #[must_use]
    pub fn with_statuses(mut self, combined: &CombinedStatus) -> Self {
        self.commit.status = StatusMark::new(combined, format!("{}#{CHECKS_ANCHOR}", self.commit.url));
        self.checks = ChecksView::new(&combined.statuses, &[]);
        self
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn file(index: usize, path: &str, additions: usize, deletions: usize) -> FileDiffView {
        let split = path.rfind('/').map_or(0, |i| i + 1);
        FileDiffView {
            anchor: format!("diff-{index}"),
            path: path.to_owned(),
            directory: path[..split].to_owned(),
            name: path[split..].to_owned(),
            old_path: None,
            status: "Modified",
            status_key: "modified",
            additions,
            deletions,
            stats_known: true,
            view_url: None,
            body: DiffBodyView::NoContentChanges,
        }
    }

    fn outline(rows: &[FileTreeRow]) -> Vec<String> {
        rows.iter().map(|row| format!("{}{}{}", "  ".repeat(row.depth), row.name, if row.file.is_none() { "/" } else { "" })).collect()
    }

    #[test]
    fn tree_merges_single_folder_chains_and_lists_folders_first() {
        let files = [
            file(0, "docs/plugins/computer-use.md", 1, 1),
            file(1, "plugins/computer-use/overlay/font.ts", 1, 0),
            file(2, "plugins/computer-use/overlay/Assets.d.ts", 1, 0),
            file(3, "plugins/computer-use/index.ts", 1, 0),
            file(4, "README.md", 1, 0),
        ];
        let rows = file_tree(&files);
        assert_eq!(
            outline(&rows),
            [
                "docs/plugins/",
                "  computer-use.md",
                "plugins/computer-use/",
                "  overlay/",
                "    Assets.d.ts",
                "    font.ts",
                "  index.ts",
                "README.md",
            ]
        );
        assert_eq!(rows[2].path, "plugins/computer-use");
        assert_eq!(rows[3].path, "plugins/computer-use/overlay");
        assert_eq!(rows[4].file.as_ref().map(|leaf| leaf.anchor.as_str()), Some("diff-2"));
    }

    /// `count` lines `{prefix}{n}`, each ending in a newline.
    pub(crate) fn numbered(prefix: &str, count: usize) -> String {
        (0..count).map(|n| format!("{prefix}{n}\n")).collect::<Vec<_>>().concat()
    }

    fn kinds(text: &DiffTextView) -> Vec<&'static str> {
        text.blocks.iter().flat_map(Block::rows).map(|row| row.kind).collect()
    }

    #[test]
    fn diff_rows_are_hunk_headers_lines_and_eof_notes_in_order() {
        let changes = rbit_git::diff::diff_bytes(b"a\nb", b"a\nc");
        let text = DiffTextView::new(&changes.hunks);
        assert_eq!(kinds(&text), ["hunk", "ctx", "del", "eof", "add", "eof"]);
        let rows: Vec<&DiffLineView> = text.blocks.iter().flat_map(Block::rows).collect();
        assert_eq!(rows[0].text, changes.hunks[0].header);
        assert!(rows[0].old_number.is_empty() && !rows[0].is_line());
        assert_eq!((rows[2].old_number.as_str(), rows[2].new_number.as_str(), rows[2].text.as_str()), ("2", "", "b"));
        assert_eq!((rows[4].old_number.as_str(), rows[4].new_number.as_str(), rows[4].text.as_str()), ("", "2", "c"));
        assert_eq!(rows[3].text, NO_NEWLINE);
        assert_eq!(text.width_classes, "diff-w1 cols-16");
    }

    #[test]
    fn a_lone_carriage_return_stays_inside_its_row() {
        let changes = rbit_git::diff::diff_bytes(b"foo\rbar\r\n", b"foo\rbaz\r\n");
        let text = DiffTextView::new(&changes.hunks);
        let lines: Vec<&str> = text.blocks.iter().flat_map(Block::rows).filter(|row| row.is_line()).map(|row| row.text.as_str()).collect();
        assert_eq!(lines, ["foo\u{240D}bar", "foo\u{240D}baz"], "shown, not a second line over the next row");
    }

    #[test]
    fn long_diffs_split_into_full_blocks_holding_every_row_once() {
        let old = numbered("", 300);
        let new = numbered("x", 300);
        let changes = rbit_git::diff::diff_bytes(old.as_bytes(), new.as_bytes());
        let text = DiffTextView::new(&changes.hunks);
        let lengths: Vec<usize> = text.blocks.iter().map(Block::len).collect();
        let expected_rows = changes.hunks.len() + changes.hunks.iter().map(|hunk| hunk.lines.len()).sum::<usize>();
        assert_eq!(lengths.iter().sum::<usize>(), expected_rows);
        assert!(lengths.iter().rev().skip(1).all(|&len| len == crate::views::blocks::BLOCK_ROWS));
        let texts: Vec<&str> = text.blocks.iter().flat_map(Block::rows).filter(|row| row.is_line()).map(|row| row.text.as_str()).collect();
        let source: Vec<&str> = changes.hunks.iter().flat_map(|hunk| &hunk.lines).map(|line| line.text.as_str()).collect();
        assert_eq!(texts, source);
        assert_eq!(text.width_classes, "diff-w3 cols-24");
    }

    #[test]
    fn number_columns_fit_the_largest_line_number() {
        let hunk = |old: usize, new: usize| Hunk {
            header: String::new(),
            lines: vec![DiffLine {
                kind: LineKind::Context,
                old_number: Some(old),
                new_number: Some(new),
                text: String::new(),
                missing_newline: false,
            }],
        };
        assert_eq!(number_class(&[]), "diff-w1");
        assert_eq!(number_class(&[hunk(9, 9)]), "diff-w1");
        assert_eq!(number_class(&[hunk(9, 10)]), "diff-w2");
        assert_eq!(number_class(&[hunk(1, 2), hunk(99_999, 3)]), "diff-w5");
        assert_eq!(number_class(&[hunk(123_456_789, 1)]), "diff-w7");
    }

    #[test]
    fn diffs_are_as_wide_as_their_widest_line() {
        let hunk = |texts: &[&str]| Hunk {
            header: "@@ -1 +1 @@".to_owned(),
            lines: texts
                .iter()
                .map(|text| DiffLine {
                    kind: LineKind::Added,
                    old_number: None,
                    new_number: Some(1),
                    text: (*text).to_owned(),
                    missing_newline: false,
                })
                .collect(),
        };
        assert_eq!(column_class(&[]), "cols-8");
        assert_eq!(column_class(&[hunk(&["x"])]), "cols-16", "the hunk header counts too");
        assert_eq!(column_class(&[hunk(&[&"y".repeat(17)])]), "cols-24");
        assert_eq!(column_class(&[hunk(&["a"]), hunk(&[&"z".repeat(1000)])]), "cols-256");
        assert_eq!(width_classes(&[hunk(&["a"])]), "diff-w1 cols-16");
    }

    #[derive(Template)]
    #[template(source = r#"{% include "repo/file_diffs.html" %}"#, ext = "html")]
    struct Diffs {
        files: Vec<FileDiffView>,
        additions: usize,
        deletions: usize,
        truncated: bool,
    }

    #[test]
    fn diffs_render_rows_in_exact_blocks_without_tables_or_inline_styles() {
        let old = numbered("", 200);
        let new = format!("{}<b>\n", old.replacen("5\n", "five\n", 1));
        let changes = rbit_git::diff::diff_bytes(old.as_bytes(), new.as_bytes());
        let mut view = file(0, "src/a.rs", changes.additions, changes.deletions);
        view.body = DiffBodyView::Text(DiffTextView::new(&changes.hunks));
        let html = Diffs { files: vec![view], additions: 2, deletions: 1, truncated: false }.render().unwrap();
        assert!(!html.contains("<table") && !html.contains("style="));
        assert!(html.contains(r#"<div class="diff-view diff-w3 cols-24">"#));
        assert!(html.contains(r#"<div class="diff-del"><span class="num">6</span><span class="num"></span>5</div>"#));
        assert!(html.contains(r#"<div class="diff-add"><span class="num"></span><span class="num">201</span>&#60;b&#62;</div>"#));
        // Each block says how many rows it holds, and holds exactly that many.
        let mut total = 0;
        for block in html.split(r#"<div class="vblock" data-rows=""#).skip(1) {
            let (rows, body) = block.split_once("\">").unwrap();
            let rows: usize = rows.parse().unwrap();
            let body = &body[..body.find('\n').unwrap_or(body.len())];
            assert_eq!(body.matches("<div class=\"diff-").count(), rows);
            total += rows;
        }
        assert_eq!(total, changes.hunks.len() + changes.hunks.iter().map(|hunk| hunk.lines.len()).sum::<usize>());
    }

    #[test]
    fn diffstat_splits_the_bar_and_keeps_both_sides_visible() {
        assert_eq!(file(0, "a", 0, 0).diffstat(), ["none"; 5]);
        assert_eq!(file(0, "a", 1, 0).diffstat(), ["add", "none", "none", "none", "none"]);
        assert_eq!(file(0, "a", 100, 100).diffstat(), ["add", "add", "add", "del", "del"]);
        assert_eq!(file(0, "a", 500, 1).diffstat(), ["add", "add", "add", "add", "del"]);
        assert_eq!(file(0, "a", 1, 500).diffstat(), ["add", "del", "del", "del", "del"]);
    }
}
