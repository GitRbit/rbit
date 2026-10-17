//! The single-commit page: message, metadata and file diffs.

use std::collections::BTreeMap;

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::{
    ChangeStatus, CommitDetail, FileDiff, FileDiffBody,
    diff::{Hunk, LineKind},
};

use rbit_services::statuses::CombinedStatus;

use crate::{
    urls::RepoUrls,
    views::{
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

#[derive(Debug, Clone)]
pub struct DiffLineView {
    /// CSS modifier: `add`, `del` or `ctx`.
    pub kind: &'static str,
    pub sign: &'static str,
    pub old_number: String,
    pub new_number: String,
    pub text: String,
    /// Shows Git's `\ No newline at end of file` marker after this line.
    pub missing_newline: bool,
}

#[derive(Debug, Clone)]
pub struct HunkView {
    pub header: String,
    pub lines: Vec<DiffLineView>,
}

impl From<&Hunk> for HunkView {
    fn from(hunk: &Hunk) -> Self {
        let number = |n: Option<usize>| n.map(|n| n.to_string()).unwrap_or_default();
        Self {
            header: hunk.header.clone(),
            lines: hunk
                .lines
                .iter()
                .map(|line| {
                    let (kind, sign) = match line.kind {
                        LineKind::Added => ("add", "+"),
                        LineKind::Removed => ("del", "-"),
                        LineKind::Context => ("ctx", " "),
                    };
                    DiffLineView {
                        kind,
                        sign,
                        old_number: number(line.old_number),
                        new_number: number(line.new_number),
                        text: line.text.clone(),
                        missing_newline: line.missing_newline,
                    }
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DiffBodyView {
    Text(Vec<HunkView>),
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
            FileDiffBody::Text(hunks) => DiffBodyView::Text(hunks.iter().map(HunkView::from).collect()),
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
mod tests {
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

    #[test]
    fn diffstat_splits_the_bar_and_keeps_both_sides_visible() {
        assert_eq!(file(0, "a", 0, 0).diffstat(), ["none"; 5]);
        assert_eq!(file(0, "a", 1, 0).diffstat(), ["add", "none", "none", "none", "none"]);
        assert_eq!(file(0, "a", 100, 100).diffstat(), ["add", "add", "add", "del", "del"]);
        assert_eq!(file(0, "a", 500, 1).diffstat(), ["add", "add", "add", "add", "del"]);
        assert_eq!(file(0, "a", 1, 500).diffstat(), ["add", "del", "del", "del", "del"]);
    }
}
