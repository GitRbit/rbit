//! The single-commit page: message, metadata and file diffs.

use askama::Template;
use askama_web::WebTemplate;
use rustit_git::{
    ChangeStatus, CommitDetail, FileDiff, FileDiffBody,
    diff::{Hunk, LineKind},
};

use crate::{
    urls::RepoUrls,
    views::{
        layout::Page,
        repo::{CommitView, RepoHeader},
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
    pub old_path: Option<String>,
    pub status: &'static str,
    pub additions: usize,
    pub deletions: usize,
    /// False when the file wasn't diffed; the counts are then hidden.
    pub stats_known: bool,
    /// View the file as of this commit (absent for deletions).
    pub view_url: Option<String>,
    pub body: DiffBodyView,
}

impl FileDiffView {
    fn new(index: usize, file: &FileDiff, commit_id: &str, urls: &RepoUrls) -> Self {
        let status = match file.status {
            ChangeStatus::Added => "Added",
            ChangeStatus::Deleted => "Deleted",
            ChangeStatus::Modified => "Modified",
            ChangeStatus::Renamed => "Renamed",
            ChangeStatus::Copied => "Copied",
        };
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
            old_path: file.old_path.clone(),
            status,
            additions: file.additions,
            deletions: file.deletions,
            stats_known: file.stats_known,
            view_url: (file.status != ChangeStatus::Deleted).then(|| urls.blob(commit_id, &file.path)),
            body,
        }
    }
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
            files: detail.files.iter().enumerate().map(|(i, f)| FileDiffView::new(i, f, &commit.id, urls)).collect(),
            additions: detail.additions(),
            deletions: detail.deletions(),
            truncated: detail.truncated,
        }
    }
}
