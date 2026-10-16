//! The blame view.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rustit_git::blame::BlameHunk;

use crate::{
    urls::RepoUrls,
    views::{
        layout::Page,
        repo::{CommitView, Crumb, RefSwitcher, RepoHeader},
    },
};

/// One line of the blamed file.
#[derive(Debug, Clone)]
pub struct BlameLine {
    /// 1-based line number (also the `#L…` anchor).
    pub number: usize,
    /// Highlighted (or escaped) HTML of the line.
    pub html: String,
}

/// A run of lines introduced by one commit, headed by that commit.
#[derive(Debug, Clone)]
pub struct BlameHunkView {
    pub commit: CommitView,
    /// The path in that commit, when the file was renamed since.
    pub source_path: Option<String>,
    pub lines: Vec<BlameLine>,
}

impl BlameHunkView {
    /// Pairs each hunk with its lines; `lines_html[i]` is line `i + 1`. Lines the
    /// highlighter did not produce (a trailing line without content) render empty.
    pub fn build(hunks: &[BlameHunk], lines_html: &[String], urls: &RepoUrls) -> Vec<Self> {
        hunks
            .iter()
            .map(|hunk| Self {
                commit: CommitView::new(&hunk.commit, urls),
                source_path: hunk.source_path.clone(),
                lines: (hunk.start_line..hunk.start_line + hunk.line_count)
                    .map(|number| BlameLine { number, html: lines_html.get(number - 1).cloned().unwrap_or_default() })
                    .collect(),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum BlameContent {
    Hunks(Vec<BlameHunkView>),
    Binary,
    TooLarge,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/blame.html")]
pub struct BlamePage {
    pub page: Page,
    pub header: RepoHeader,
    pub switcher: RefSwitcher,
    pub breadcrumbs: Vec<Crumb>,
    pub latest: CommitView,
    pub file_name: String,
    /// The normal file view.
    pub blob_url: String,
    pub raw_url: String,
    pub content: BlameContent,
}

#[cfg(test)]
mod tests {
    use rustit_core::Timestamp;
    use rustit_git::CommitSummary;

    use super::*;

    fn commit(id: &str) -> CommitSummary {
        CommitSummary {
            id: id.repeat(40),
            short_id: id.repeat(7),
            summary: format!("commit {id}"),
            body: None,
            author_name: "Ada".into(),
            author_email: "ada@example.com".into(),
            authored_at: Timestamp(0),
            parent_ids: Vec::new(),
        }
    }

    #[test]
    fn hunks_take_their_lines_in_order() {
        let hunks = [
            BlameHunk { commit: commit("a"), start_line: 1, line_count: 2, source_path: None },
            BlameHunk { commit: commit("b"), start_line: 3, line_count: 2, source_path: Some("old.rs".into()) },
        ];
        let lines: Vec<String> = ["l1", "l2", "l3"].map(str::to_owned).to_vec();
        let views = BlameHunkView::build(&hunks, &lines, &RepoUrls::new("o", "r"));
        assert_eq!(views[0].lines.iter().map(|l| l.number).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(views[1].lines[0].html, "l3");
        assert_eq!(views[1].lines[1].html, "", "missing lines render empty");
        assert_eq!(views[1].commit.url, format!("/o/r/commit/{}", "b".repeat(40)));
    }
}
