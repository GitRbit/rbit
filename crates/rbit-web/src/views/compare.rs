//! The branch compare page.
//!
//! Milestone 2 workstream R (repository essentials) wrote this file; wave B's pull request
//! workstream (P) owns it now and added the repository pickers (cross-fork compare) and the
//! "Create pull request" button.

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::{RefKind, RefSummary, compare::Comparison};

use crate::{
    urls::RepoUrls,
    views::{
        // `DiffBodyView` is matched by `repo/file_diffs.html`.
        commit::{DiffBodyView, FileDiffView},
        layout::Page,
        repo::{CommitView, RepoHeader},
    },
};

/// One choice in the base/head pickers.
#[derive(Debug, Clone)]
pub struct RefOption {
    pub name: String,
    pub is_tag: bool,
}

/// The base and head pickers (a plain GET form, so it works without JavaScript).
#[derive(Debug, Clone)]
pub struct ComparePicker {
    pub branches_only: bool,
    pub action_url: String,
    pub options: Vec<RefOption>,
    pub base: String,
    pub head: String,
    /// `owner/name` of every readable repository of the fork network; the repository
    /// selects are shown only when there is more than one.
    pub repositories: Vec<String>,
    pub base_repo: String,
    pub head_repo: String,
}

impl ComparePicker {
    pub fn new(urls: &RepoUrls, refs: &[RefSummary], base: &str, head: &str) -> Self {
        let full_name = urls.home().trim_start_matches('/').to_owned();
        Self {
            branches_only: false,
            action_url: format!("{}/compare", urls.home()),
            options: refs.iter().take(50).map(|r| RefOption { name: r.name.clone(), is_tag: r.kind == RefKind::Tag }).collect(),
            base: base.to_owned(),
            head: head.to_owned(),
            repositories: vec![full_name.clone()],
            base_repo: full_name.clone(),
            head_repo: full_name,
        }
    }

    /// Offers the repositories of the fork network, with `head_repo` selected for the head.
    #[must_use]
    pub fn with_network(mut self, repositories: Vec<String>, head_repo: &str) -> Self {
        if !repositories.is_empty() {
            self.repositories = repositories;
        }
        head_repo.clone_into(&mut self.head_repo);
        self
    }
}

/// What a comparison found, apart from the file diffs (top-level fields of the page,
/// shared with the commit page's diff template).
#[derive(Debug, Clone)]
pub struct CompareSummary {
    pub base: String,
    /// The head as written in the URL (`branch`, `owner:branch`, …).
    pub head: String,
    pub ahead: usize,
    pub behind: usize,
    /// "10000+" style when the count was capped.
    pub ahead_label: String,
    pub behind_label: String,
    pub merge_base: Option<CommitLink>,
    pub commits: Vec<CommitView>,
    /// More commits are ahead than listed.
    pub more_commits: bool,
    /// The same comparison the other way round (same repository only).
    pub swap_url: Option<String>,
    /// "Create pull request", when the head has commits the base lacks.
    pub create_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommitLink {
    pub short_id: String,
    pub url: String,
}

impl CompareSummary {
    /// The summary of `comparison`; commit links point at `head_urls` (the head
    /// repository, which has every commit ahead).
    pub fn new(comparison: &Comparison, base: &str, head: &str, urls: &RepoUrls, head_urls: &RepoUrls, same_repository: bool) -> Self {
        let label = |count: usize| {
            if count >= rbit_git::compare::MAX_COUNTED_COMMITS { format!("{count}+") } else { count.to_string() }
        };
        Self {
            base: base.to_owned(),
            head: head.to_owned(),
            ahead: comparison.ahead,
            behind: comparison.behind,
            ahead_label: label(comparison.ahead),
            behind_label: label(comparison.behind),
            merge_base: comparison
                .merge_base
                .as_ref()
                .map(|id| CommitLink { short_id: id[..7.min(id.len())].to_owned(), url: urls.commit(id) }),
            commits: comparison.commits.iter().map(|c| CommitView::new(c, head_urls)).collect(),
            more_commits: comparison.ahead > comparison.commits.len(),
            swap_url: same_repository.then(|| urls.compare(head, base)),
            create_url: None,
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/compare.html")]
pub struct ComparePage {
    pub page: Page,
    pub header: RepoHeader,
    pub picker: ComparePicker,
    /// `None` until both sides are chosen.
    pub summary: Option<CompareSummary>,
    pub files: Vec<FileDiffView>,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
}

impl ComparePage {
    /// Diff views of `comparison`, linking files to their version at `head` in `urls`.
    #[must_use]
    pub fn with_comparison(mut self, comparison: &Comparison, urls: &RepoUrls) -> Self {
        self.files = FileDiffView::list(&comparison.files, &comparison.head.id, urls);
        self.additions = comparison.additions();
        self.deletions = comparison.deletions();
        self.truncated = comparison.truncated;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_lists_the_network_and_keeps_the_head_repository() {
        let urls = RepoUrls::new("alice", "demo");
        let picker = ComparePicker::new(&urls, &[], "main", "feature");
        assert_eq!((picker.base_repo.as_str(), picker.repositories.len()), ("alice/demo", 1));
        let picker = picker.with_network(vec!["alice/demo".into(), "bob/demo".into()], "bob/demo");
        assert_eq!((picker.head_repo.as_str(), picker.repositories.len()), ("bob/demo", 2));
    }
}
