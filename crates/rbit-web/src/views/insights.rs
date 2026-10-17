//! Language breakdown and last-commit-per-entry view models.
//!
//! The language bar is drawn as inline SVG: the strict CSP forbids `style=` attributes, so
//! proportional widths and colours are expressed as SVG `x`/`width`/`fill` attributes.

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::{CommitSummary, LanguageStats};

use crate::urls::RepoUrls;

/// SVG user units spanning the whole bar; segment geometry is in percent of it.
const BAR_WIDTH: f64 = 100.0;

/// One language in the bar and the legend.
#[derive(Debug, Clone)]
pub struct LanguageSegment {
    pub name: String,
    /// `#rrggbb`, used as an SVG `fill`.
    pub color: String,
    /// e.g. `93.3%`.
    pub percent: String,
    /// SVG `x` and `width` attributes within a 100-unit-wide `viewBox`.
    pub x: String,
    pub width: String,
}

/// The sidebar "Languages" section.
#[derive(Debug, Clone)]
pub struct LanguageBar {
    pub segments: Vec<LanguageSegment>,
    /// Accessible description of the whole bar, e.g. `Rust 93.3%, HTML 6.7%`.
    pub label: String,
}

impl LanguageBar {
    /// `None` when nothing countable was found, so the section is omitted.
    pub fn new(stats: &LanguageStats) -> Option<Self> {
        if stats.is_empty() {
            return None;
        }
        let mut x = 0.0;
        let segments: Vec<LanguageSegment> = stats
            .languages
            .iter()
            .map(|share| {
                let width = share.percent / 100.0 * BAR_WIDTH;
                let segment = LanguageSegment {
                    name: share.name.clone(),
                    color: share.color.clone(),
                    percent: format_percent(share.percent),
                    x: format!("{x:.3}"),
                    width: format!("{width:.3}"),
                };
                x += width;
                segment
            })
            .collect();
        let label = segments.iter().map(|s| format!("{} {}", s.name, s.percent)).collect::<Vec<_>>().join(", ");
        Some(Self { segments, label })
    }
}

/// One decimal place, like GitHub; tiny shares never read as `0.0%`.
fn format_percent(percent: f64) -> String {
    if percent > 0.0 && percent < 0.1 { "<0.1%".to_owned() } else { format!("{percent:.1}%") }
}

/// The DOM id of an entry's lazily filled commit cell; the tree page and the fragment
/// both derive it from the entry's position in the (deterministically sorted) listing.
pub fn entry_commit_slot(index: usize) -> String {
    format!("entry-commit-{index}")
}

/// The last commit of one tree entry, swapped into its row out of band.
#[derive(Debug, Clone)]
pub struct EntryCommitCell {
    pub slot_id: String,
    /// `None` when the bounded history walk did not reach the entry's last change.
    pub commit: Option<EntryCommit>,
}

#[derive(Debug, Clone)]
pub struct EntryCommit {
    pub summary: String,
    /// The commit's own page.
    pub url: String,
    pub when: String,
    pub when_iso: String,
}

impl EntryCommit {
    pub fn new(commit: &CommitSummary, urls: &RepoUrls) -> Self {
        Self {
            summary: commit.summary.clone(),
            url: urls.commit(&commit.id),
            when: commit.authored_at.relative(),
            when_iso: commit.authored_at.iso8601(),
        }
    }
}

/// `GET /{owner}/{repo}/tree-commits/{commit}/{path…}`: only out-of-band cells, loaded
/// by htmx after the tree page has painted.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/entry_commits.html")]
pub struct EntryCommitsFragment {
    pub cells: Vec<EntryCommitCell>,
}

#[cfg(test)]
mod tests {
    use rbit_git::LanguageShare;

    use super::*;

    fn share(name: &str, percent: f64) -> LanguageShare {
        LanguageShare { name: name.into(), color: "#000000".into(), bytes: 1, percent }
    }

    #[test]
    fn segments_are_laid_end_to_end() {
        let stats = LanguageStats { total_bytes: 3, languages: vec![share("Rust", 75.0), share("HTML", 20.0), share("Other", 5.0)] };
        let bar = LanguageBar::new(&stats).unwrap();
        let geometry: Vec<_> = bar.segments.iter().map(|s| (s.x.as_str(), s.width.as_str())).collect();
        assert_eq!(geometry, [("0.000", "75.000"), ("75.000", "20.000"), ("95.000", "5.000")]);
        assert_eq!(bar.label, "Rust 75.0%, HTML 20.0%, Other 5.0%");
    }

    #[test]
    fn empty_stats_have_no_bar() {
        assert!(LanguageBar::new(&LanguageStats::default()).is_none());
    }

    #[test]
    fn fragment_is_only_out_of_band_cells() {
        let commit = EntryCommit {
            summary: "Fix <bug>".into(),
            url: "/a/b/commit/0123".into(),
            when: "2 days ago".into(),
            when_iso: "2026-01-01T00:00:00Z".into(),
        };
        let fragment = EntryCommitsFragment {
            cells: vec![
                EntryCommitCell { slot_id: entry_commit_slot(0), commit: Some(commit) },
                EntryCommitCell { slot_id: entry_commit_slot(1), commit: None },
            ],
        };
        let html = fragment.render().unwrap();
        assert_eq!(html.matches("hx-swap-oob=\"true\"").count(), 2);
        assert!(html.contains(r#"id="entry-commit-0""#) && html.contains(r#"id="entry-commit-1""#));
        assert!(html.contains(r#"href="/a/b/commit/0123""#));
        assert!(html.contains("Fix &#60;bug&#62;") || html.contains("Fix &lt;bug&gt;"), "{html}");
    }

    #[test]
    fn percentages_have_one_decimal() {
        assert_eq!(format_percent(93.333), "93.3%");
        assert_eq!(format_percent(0.04), "<0.1%");
        assert_eq!(format_percent(100.0), "100.0%");
    }
}
