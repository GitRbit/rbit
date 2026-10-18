//! The blame view.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Blocks
//!
//! A blamed file renders as one run of virtual blocks (`.vblock`, see
//! [`blocks`](crate::views::blocks)) inside a single horizontal scroller, like the file
//! viewer, so a 20,000-line file with thousands of small hunks lays out a block or two at
//! a time instead of every hunk. Each hunk is a header row group followed by its lines:
//!
//! - **The header is [`HEADER_ROWS`] rows tall**, exactly (its hairline included), so it
//!   counts in a block's `data-rows` like any line and the placeholder stays exact. Two
//!   rows rather than one keep the commit link a comfortable tap target on a phone.
//! - **Consecutive hunks pack into one block** up to [`BLOCK_ROWS`] rows, so many small
//!   hunks share a containment boundary.
//! - **A header never ends a block**: when fewer than `HEADER_ROWS + 1` rows are left, the
//!   block ends early and the hunk starts the next one, header and first line together.
//! - **A hunk longer than what is left splits across blocks**, its header only on the
//!   first; the blocks join seamlessly, so the continuation reads as the same hunk.
//!
//! Blocks are built from borrowed hunks at render time ([`BlamePage::blocks`]), so the
//! handler keeps passing [`BlameHunkView`]s.

use std::{ops::Range, sync::Arc};

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::blame::BlameHunk;

use crate::{
    urls::RepoUrls,
    views::{
        blocks::{BLOCK_ROWS, column_class, markup_columns},
        layout::Page,
        repo::{CommitView, Crumb, RefSwitcher, RepoHeader},
    },
};

/// One line of the blamed file.
#[derive(Debug, Clone, Copy)]
pub struct BlameLine<'a> {
    /// 1-based line number (also the `#L…` anchor).
    pub number: usize,
    /// Highlighted (or escaped) HTML of the line.
    pub html: &'a str,
}

/// A run of lines introduced by one commit, headed by that commit.
#[derive(Debug, Clone)]
pub struct BlameHunkView {
    pub commit: CommitView,
    /// The path in that commit, when the file was renamed since.
    pub source_path: Option<String>,
    lines: Range<usize>,
    lines_html: Arc<[String]>,
}

impl BlameHunkView {
    /// Pairs each hunk with its lines; `lines_html[i]` is line `i + 1`. Lines the
    /// highlighter did not produce (a trailing line without content) render empty.
    pub fn build(hunks: &[BlameHunk], lines_html: &Arc<[String]>, urls: &RepoUrls) -> Vec<Self> {
        hunks
            .iter()
            .map(|hunk| Self {
                commit: CommitView::new(&hunk.commit, urls),
                source_path: hunk.source_path.clone(),
                lines: hunk.start_line..hunk.start_line + hunk.line_count,
                lines_html: Arc::clone(lines_html),
            })
            .collect()
    }

    /// Borrows markup from the cached allocation only as the page renders.
    pub fn lines(&self) -> impl ExactSizeIterator<Item = BlameLine<'_>> {
        self.lines.clone().map(|number| BlameLine { number, html: self.lines_html.get(number - 1).map_or("", String::as_str) })
    }
}

/// Height of a hunk header, in rows (`--row-h`); app.css's `.blame-hunk` matches it.
pub const HEADER_ROWS: usize = 2;

/// One entry of a [`BlameBlock`]: a hunk's header or one of its lines.
#[derive(Debug, Clone, Copy)]
pub enum BlameRow<'a> {
    /// The commit that introduced the lines below; [`HEADER_ROWS`] rows tall.
    Header(&'a BlameHunkView),
    /// One line, one row tall.
    Line(BlameLine<'a>),
}

impl BlameRow<'_> {
    /// Height in rows.
    #[must_use]
    pub fn height(&self) -> usize {
        match self {
            Self::Header(_) => HEADER_ROWS,
            Self::Line(_) => 1,
        }
    }
}

/// A virtual block of hunk headers and lines, at most [`BLOCK_ROWS`] rows tall.
#[derive(Debug, Clone, Default)]
pub struct BlameBlock<'a> {
    entries: Vec<BlameRow<'a>>,
    rows: usize,
}

impl<'a> BlameBlock<'a> {
    fn push(&mut self, entry: BlameRow<'a>) {
        self.rows += entry.height();
        self.entries.push(entry);
    }

    /// Headers and lines, in order.
    #[must_use]
    pub fn entries(&self) -> &[BlameRow<'a>] {
        &self.entries
    }

    /// Height in rows, headers counted: rendered as `data-rows`.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }
}

/// Packs hunks into blocks; see the module documentation for the rules.
pub fn blame_blocks(hunks: &[BlameHunkView]) -> Vec<BlameBlock<'_>> {
    let mut blocks = Vec::new();
    let mut current = BlameBlock::default();
    for hunk in hunks {
        // The header and the hunk's first line stay together.
        if current.rows + HEADER_ROWS + usize::from(!hunk.lines.is_empty()) > BLOCK_ROWS {
            blocks.push(std::mem::take(&mut current));
        }
        current.push(BlameRow::Header(hunk));
        for line in hunk.lines() {
            if current.rows == BLOCK_ROWS {
                blocks.push(std::mem::take(&mut current));
            }
            current.push(BlameRow::Line(line));
        }
    }
    if current.rows > 0 {
        blocks.push(current);
    }
    blocks
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

impl BlamePage {
    /// The hunks in virtual blocks; none unless the content is [`BlameContent::Hunks`].
    pub fn blocks(&self) -> Vec<BlameBlock<'_>> {
        match &self.content {
            BlameContent::Hunks(hunks) => blame_blocks(hunks),
            BlameContent::Binary | BlameContent::TooLarge => Vec::new(),
        }
    }

    /// The code's minimum width, from its widest line ([`column_class`]).
    pub fn code_columns(&self) -> String {
        let widest = match &self.content {
            BlameContent::Hunks(hunks) => {
                hunks.iter().flat_map(BlameHunkView::lines).map(|line| markup_columns(line.html)).max().unwrap_or(0)
            }
            BlameContent::Binary | BlameContent::TooLarge => 0,
        };
        column_class(widest)
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::Timestamp;
    use rbit_git::CommitSummary;

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
        let lines: Arc<[String]> = ["l1", "l2", "l3"].map(str::to_owned).into();
        let views = BlameHunkView::build(&hunks, &lines, &RepoUrls::new("o", "r"));
        assert!(Arc::ptr_eq(&views[0].lines_html, &lines));
        assert!(Arc::ptr_eq(&views[1].lines_html, &lines));
        assert_eq!(views[0].lines().map(|l| l.number).collect::<Vec<_>>(), [1, 2]);
        let mut second = views[1].lines();
        let third = second.next().unwrap();
        assert_eq!(third.html, "l3");
        assert_eq!(third.html.as_ptr(), lines[2].as_ptr(), "rows borrow the cached strings");
        assert_eq!(second.next().unwrap().html, "", "missing lines render empty");
        assert_eq!(views[1].commit.url, format!("/o/r/commit/{}", "b".repeat(40)));
    }

    /// Consecutive hunks of `sizes` lines each, alternating between two commits.
    fn hunks(sizes: &[usize]) -> Vec<BlameHunkView> {
        let mut start = 1;
        let hunks: Vec<BlameHunk> = sizes
            .iter()
            .enumerate()
            .map(|(index, &line_count)| {
                let hunk =
                    BlameHunk { commit: commit(if index % 2 == 0 { "a" } else { "b" }), start_line: start, line_count, source_path: None };
                start += line_count;
                hunk
            })
            .collect();
        let lines: Arc<[String]> = (1..start).map(|n| format!("<i>line {n}</i>")).collect();
        BlameHunkView::build(&hunks, &lines, &RepoUrls::new("o", "r"))
    }

    /// Checks every packing rule and returns the blocks' heights.
    fn check(sizes: &[usize]) -> Vec<usize> {
        let views = hunks(sizes);
        let blocks = blame_blocks(&views);
        let mut lines = Vec::new();
        let mut headers = 0;
        for block in &blocks {
            let height: usize = block.entries().iter().map(BlameRow::height).sum();
            assert_eq!(block.rows(), height, "data-rows is the height, headers counted");
            assert!(block.rows() > 0 && block.rows() <= BLOCK_ROWS);
            for (index, entry) in block.entries().iter().enumerate() {
                match entry {
                    BlameRow::Header(hunk) => {
                        assert!(std::ptr::eq(*hunk, std::ptr::from_ref(&views[headers])), "headers in hunk order");
                        if let Some(first) = hunk.lines().next() {
                            assert!(
                                matches!(block.entries().get(index + 1), Some(BlameRow::Line(line)) if line.number == first.number),
                                "a header is followed by its first line in the same block"
                            );
                        }
                        headers += 1;
                    }
                    BlameRow::Line(line) => lines.push(line.number),
                }
            }
        }
        assert_eq!(headers, views.len(), "one header per hunk");
        let total: usize = sizes.iter().sum();
        assert_eq!(lines, (1..=total).collect::<Vec<_>>(), "every line exactly once, in order");
        blocks.iter().map(BlameBlock::rows).collect()
    }

    #[test]
    fn small_hunks_pack_into_shared_blocks() {
        // 42 one-line hunks of three rows fill 126 rows; a 43rd would not fit.
        let heights = check(&[1; 100]);
        assert_eq!(heights, [126, 126, 48]);
    }

    #[test]
    fn long_hunks_split_with_the_header_only_on_the_first_block() {
        assert_eq!(check(&[300]), [128, 128, 300 - 126 - 128]);
        let views = hunks(&[300]);
        let blocks = blame_blocks(&views);
        assert!(matches!(blocks[0].entries()[0], BlameRow::Header(_)));
        assert!(blocks[1..].iter().all(|block| matches!(block.entries()[0], BlameRow::Line(_))));
    }

    #[test]
    fn a_header_never_ends_a_block() {
        // 126 rows used: a header and its first line (3 rows) start the next block.
        assert_eq!(check(&[124, 5]), [126, 7]);
        // 125 rows used: exactly room for a header and one line.
        assert_eq!(check(&[123, 5]), [128, 4]);
        assert_eq!(check(&[]), [0_usize; 0]);
        check(&[1, 127, 2, 1, 400, 3, 128, 126, 1]);
    }

    fn page(content: BlameContent) -> BlamePage {
        let urls = RepoUrls::new("o", "r");
        BlamePage {
            page: Page {
                site_name: "rbit".to_owned(),
                title: None,
                viewer: None,
                path: "/o/r/blame/main/a.rs".to_owned(),
                theme: crate::themes::Theme::System,
                has_unread_notifications: false,
            },
            header: RepoHeader::default(),
            switcher: RefSwitcher::deferred(
                &[],
                "main",
                crate::views::ref_picker::RefPickerLinks::new(&urls, &crate::views::ref_picker::RefPickerQuery::default()),
            ),
            breadcrumbs: Vec::new(),
            latest: CommitView::new(&commit("a"), &urls),
            file_name: "a.rs".to_owned(),
            blob_url: "/o/r/blob/main/a.rs".to_owned(),
            raw_url: "/o/r/raw/main/a.rs".to_owned(),
            content,
        }
    }

    #[test]
    fn page_renders_blocks_whose_rows_match_their_contents() {
        let sizes = [1, 2, 200, 1, 1, 60];
        let html = page(BlameContent::Hunks(hunks(&sizes))).render().unwrap();
        assert!(!html.contains("style="), "no inline styles under the CSP");

        let blocks: Vec<&str> = html.split(r#"<div class="vblock" data-rows=""#).skip(1).collect();
        assert_eq!(blocks.len(), 3);
        for block in blocks {
            let (rows, body) = block.split_once('"').unwrap();
            let rows: usize = rows.parse().unwrap();
            let lines = body.matches(r#"class="code-line""#).count();
            let headers = body.matches(r#"class="blame-hunk""#).count();
            assert_eq!(rows, lines + HEADER_ROWS * headers);
        }
        assert!(html.contains(r#"<div class="code-lines blame-lines cols-8">"#));
        assert_eq!(html.matches(r#"class="blame-hunk""#).count(), sizes.len());
        let total: usize = sizes.iter().sum();
        for n in 1..=total {
            assert_eq!(html.matches(&format!(r#"id="L{n}""#)).count(), 1, "line {n} once, with its id");
        }
        assert!(html.contains(r#"<span class="line-number" data-line="265"></span><span class="line-code"><i>line 265</i></span>"#));
        assert!(html.contains(&format!(r#"href="/o/r/commit/{}""#, "b".repeat(40))));
    }

    #[test]
    fn empty_files_say_so() {
        let html = page(BlameContent::Hunks(Vec::new())).render().unwrap();
        assert!(html.contains("This file is empty."));
        assert!(!html.contains("vblock"));
    }
}
