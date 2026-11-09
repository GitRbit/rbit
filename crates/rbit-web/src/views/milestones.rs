//! Milestone list, detail and form pages.
//!
//! Milestone 2 workstream I (issues) owns this file (its templates are
//! `templates/issues/milestone*.html`). Progress bars are SVG rectangles whose `width`
//! attribute is the percentage: no inline styles.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    IssueState, Timestamp,
    issue_query::{DEFAULT_QUERY, IssueQuery},
};
use rbit_localization::{
    Catalog, Locale, Localized,
    time::{CivilDate, TimeMessage},
};
use rbit_services::issues::MilestoneProgress;

use crate::views::{
    components::{FormError, Pagination},
    issues::{IssueRow, list_url},
    layout::Page,
    repo::RepoHeader,
    when::relative,
};

/// `2026-12-31` as `Dec 31, 2026` in English, `31. Dez. 2026` in German (anything else
/// as written).
pub fn human_day(day: &str, locale: Locale) -> String {
    let mut parts = day.splitn(3, '-');
    let (Some(year), Some(month), Some(date)) = (parts.next(), parts.next(), parts.next()) else { return day.to_owned() };
    let (Ok(year), Ok(month), Ok(day_of_month)) = (year.parse::<i32>(), month.parse::<u8>(), date.parse::<u8>()) else {
        return day.to_owned();
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day_of_month) {
        return day.to_owned();
    }
    let date = CivilDate { year, month, day: day_of_month };
    Catalog::new(locale).localized(TimeMessage::Date(date)).to_string()
}

/// Today as `YYYY-MM-DD` (UTC), for "past due".
fn today() -> String {
    Timestamp::now().iso8601().chars().take(10).collect()
}

/// One milestone with its progress and links.
#[derive(Debug, Clone)]
pub struct MilestoneView {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub description: String,
    pub due: Option<String>,
    pub overdue: bool,
    pub closed: bool,
    pub closed_when: Option<Localized>,
    pub open_issues: u64,
    pub closed_issues: u64,
    /// `0..=100`, for the bar's `width` attribute and the label.
    pub percent: u64,
    /// The issue list filtered by this milestone.
    pub issues_url: String,
    pub edit_url: String,
    pub state_url: String,
    pub delete_url: String,
}

impl MilestoneView {
    pub fn new(progress: &MilestoneProgress, base: &str, locale: Locale) -> Self {
        let milestone = &progress.milestone;
        let total = progress.open_issues + progress.closed_issues;
        let percent = (progress.closed_issues * 100).checked_div(total).unwrap_or(0);
        let url = format!("{base}/milestones/{}", milestone.id.0);
        let closed = milestone.state == IssueState::Closed;
        let query = IssueQuery::parse(DEFAULT_QUERY).with_milestone(Some(milestone.title.clone()));
        Self {
            id: milestone.id.0,
            title: milestone.title.clone(),
            description: milestone.description.lines().next().unwrap_or_default().to_owned(),
            due: milestone.due_on.as_deref().map(|day| human_day(day, locale)),
            overdue: !closed && milestone.due_on.as_deref().is_some_and(|due| due < today().as_str()),
            closed,
            closed_when: milestone.closed_at.map(|at| relative(at, locale)),
            open_issues: progress.open_issues,
            closed_issues: progress.closed_issues,
            percent,
            issues_url: list_url(&format!("{base}/issues"), &query),
            edit_url: format!("{url}/edit"),
            state_url: format!("{url}/state"),
            delete_url: format!("{url}/delete"),
            url,
        }
    }
}

/// `/{owner}/{repo}/milestones[?state=closed]`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/milestones.html")]
pub struct MilestonesPage {
    pub page: Page,
    pub header: RepoHeader,
    pub milestones: Vec<MilestoneView>,
    pub open: u64,
    pub closed: u64,
    pub showing_closed: bool,
    pub open_url: String,
    pub closed_url: String,
    pub can_write: bool,
    pub new_url: String,
    pub issues_url: String,
    pub labels_url: String,
}

/// The milestone form's values.
#[derive(Debug, Clone, Default)]
pub struct MilestoneForm {
    pub title: String,
    pub description: String,
    pub due_on: String,
}

/// `/{owner}/{repo}/milestones/new` and `/milestones/{id}/edit`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/milestone_form.html")]
pub struct MilestoneFormPage {
    pub page: Page,
    pub header: RepoHeader,
    pub error: FormError,
    pub form: MilestoneForm,
    pub action_url: String,
    pub cancel_url: String,
    pub is_new: bool,
}

/// `/{owner}/{repo}/milestones/{id}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/milestone.html")]
pub struct MilestonePage {
    pub page: Page,
    pub header: RepoHeader,
    pub milestone: MilestoneView,
    /// Sanitised HTML of the full description; empty without one.
    pub description_html: String,
    pub rows: Vec<IssueRow>,
    pub showing_closed: bool,
    pub open_url: String,
    pub closed_url: String,
    pub pagination: Pagination,
    pub can_write: bool,
    pub milestones_url: String,
}

#[cfg(test)]
mod tests {
    use rbit_core::{MilestoneId, RepoId};
    use rbit_services::issues::Milestone;

    use super::*;

    fn progress(open: u64, closed: u64, due: Option<&str>) -> MilestoneProgress {
        MilestoneProgress {
            milestone: Milestone {
                id: MilestoneId(3),
                repo_id: RepoId(1),
                title: "v1.0".into(),
                description: "First line\nSecond".into(),
                due_on: due.map(str::to_owned),
                state: IssueState::Open,
                closed_at: None,
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
            },
            open_issues: open,
            closed_issues: closed,
        }
    }

    #[test]
    fn progress_due_dates_and_links() {
        let view = MilestoneView::new(&progress(1, 3, Some("2000-02-29")), "/a/b", Locale::English);
        assert_eq!((view.percent, view.description.as_str()), (75, "First line"));
        assert_eq!(view.due.as_deref(), Some("Feb 29, 2000"));
        assert!(view.overdue);
        assert_eq!(view.issues_url, "/a/b/issues?q=is%3Aopen%20is%3Aissue%20milestone%3Av1%2E0");
        assert_eq!(MilestoneView::new(&progress(0, 0, None), "/a/b", Locale::English).percent, 0);
        assert!(!MilestoneView::new(&progress(0, 0, Some("9999-12-31")), "/a/b", Locale::English).overdue);
        assert_eq!(human_day("2026-13-01", Locale::English), "2026-13-01");
        assert_eq!(human_day("2026-11-08", Locale::German), "8. Nov. 2026");
        assert_eq!(human_day("2026-11-08", Locale::Korean), "2026년 11월 8일");
    }

    #[test]
    fn the_list_draws_progress_with_svg_widths() {
        let html = MilestonesPage {
            page: Page {
                locale: rbit_localization::Locale::default(),
                content_locale: rbit_localization::Locale::English,
                updates: rbit_services::updates::UpdateStatus::default(),
                site_name: "rbit".into(),
                branding: rbit_core::config::BrandingConfig::default(),
                title: None,
                viewer: None,
                path: "/a/b/milestones".into(),
                has_unread_notifications: false,
                theme: crate::themes::Theme::System,
            },
            header: RepoHeader::default(),
            milestones: vec![MilestoneView::new(&progress(1, 3, None), "/a/b", Locale::English)],
            open: 1,
            closed: 0,
            showing_closed: false,
            open_url: "/a/b/milestones".into(),
            closed_url: "/a/b/milestones?state=closed".into(),
            can_write: true,
            new_url: "/a/b/milestones/new".into(),
            issues_url: "/a/b/issues".into(),
            labels_url: "/a/b/labels".into(),
        }
        .render()
        .unwrap();
        assert!(html.contains(r#"width="75""#), "{html}");
        assert!(html.contains("75% complete"));
        assert!(!html.contains("style="));
    }
}
