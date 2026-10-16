//! Branch and tag listings.

use askama::Template;
use askama_web::WebTemplate;
use rustit_git::RefDetail;

use crate::{
    urls::RepoUrls,
    views::{
        layout::Page,
        repo::{CommitView, RepoHeader},
    },
};

/// One branch or tag row.
#[derive(Debug, Clone)]
pub struct RefRow {
    pub name: String,
    /// Browse the tree at this ref.
    pub url: String,
    pub is_default: bool,
    pub commit: CommitView,
}

impl RefRow {
    pub fn new(detail: &RefDetail, default_branch: &str, urls: &RepoUrls) -> Self {
        // Link by full ref name: a branch and a tag may share a short name.
        let full_name = match detail.kind {
            rustit_git::RefKind::Branch => format!("refs/heads/{}", detail.name),
            rustit_git::RefKind::Tag => format!("refs/tags/{}", detail.name),
        };
        Self {
            name: detail.name.clone(),
            url: urls.tree(&full_name, ""),
            is_default: detail.kind == rustit_git::RefKind::Branch && detail.name == default_branch,
            commit: CommitView::new(&detail.commit, urls),
        }
    }
}

/// Shared by the branches and tags pages; `kind` is `"branch"` or `"tag"`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/refs.html")]
pub struct RefsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub kind: &'static str,
    pub rows: Vec<RefRow>,
}
