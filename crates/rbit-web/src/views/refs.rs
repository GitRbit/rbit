//! Branch and tag listings.
//!
//! Milestone 2 workstream B (branch protection) adds the "Protected" badge: branches some
//! rule protects (`ProtectionService::protected_branches`) are marked with
//! [`RefsPage::with_protected`].

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::RefDetail;

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
    /// Source archives of the ref (shown for tags).
    pub zip_url: String,
    pub tar_url: String,
    /// The branch compared against the default branch (other branches only).
    pub compare_url: Option<String>,
    /// A branch protection rule matches this branch.
    pub protected: bool,
}

impl RefRow {
    pub fn new(detail: &RefDetail, default_branch: &str, urls: &RepoUrls) -> Self {
        // Link by full ref name: a branch and a tag may share a short name.
        let full_name = match detail.kind {
            rbit_git::RefKind::Branch => format!("refs/heads/{}", detail.name),
            rbit_git::RefKind::Tag => format!("refs/tags/{}", detail.name),
        };
        let is_default = detail.kind == rbit_git::RefKind::Branch && detail.name == default_branch;
        Self {
            compare_url: (detail.kind == rbit_git::RefKind::Branch && !is_default).then(|| urls.compare(default_branch, &detail.name)),
            name: detail.name.clone(),
            url: urls.tree(&full_name, ""),
            zip_url: urls.archive(&detail.name, ".zip"),
            tar_url: urls.archive(&detail.name, ".tar.gz"),
            is_default,
            commit: CommitView::new(&detail.commit, urls),
            protected: false,
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

impl RefsPage {
    /// Marks the branches named in `protected` (on the branches page only).
    #[must_use]
    pub fn with_protected(mut self, protected: &[String]) -> Self {
        if self.kind == "branch" {
            for row in &mut self.rows {
                row.protected = protected.contains(&row.name);
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::components::Avatar;

    fn row(name: &str) -> RefRow {
        let commit = CommitView {
            id: "1".repeat(40),
            short_id: "1111111".to_owned(),
            summary: "Initial commit".to_owned(),
            author: "Alice".to_owned(),
            avatar: Avatar::default(),
            when: "today".to_owned(),
            when_iso: String::new(),
            date: String::new(),
            browse_url: String::new(),
            url: "/acme/rocket/commit/1111111".to_owned(),
        };
        RefRow {
            name: name.to_owned(),
            url: format!("/acme/rocket/tree/refs/heads/{name}"),
            is_default: name == "main",
            commit,
            zip_url: String::new(),
            tar_url: String::new(),
            compare_url: None,
            protected: false,
        }
    }

    fn page(kind: &'static str) -> RefsPage {
        RefsPage {
            page: Page {
                site_name: "rbit".to_owned(),
                title: None,
                viewer: None,
                path: "/acme/rocket/branches".to_owned(),
                theme: crate::themes::Theme::System,
                has_unread_notifications: false,
            },
            header: RepoHeader { owner: "acme".to_owned(), name: "rocket".to_owned(), active_tab: "branches", ..RepoHeader::default() },
            kind,
            rows: vec![row("main"), row("feature")],
        }
    }

    #[test]
    fn protected_branches_carry_a_badge() {
        let protected = ["main".to_owned()];
        let branches = page("branch").with_protected(&protected);
        assert!(branches.rows[0].protected && !branches.rows[1].protected);
        let html = branches.render().unwrap();
        assert_eq!(html.matches("Protected</span>").count(), 1);
        assert!(!html.contains("style="));

        let tags = page("tag").with_protected(&protected);
        assert!(!tags.rows[0].protected, "tags are never marked");
        assert!(!page("branch").render().unwrap().contains("Protected</span>"));
    }
}
