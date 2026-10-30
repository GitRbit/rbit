//! The labels page.
//!
//! Milestone 2 workstream I (issues) owns this file (its template is
//! `templates/issues/labels.html`).

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::issue_query::{DEFAULT_QUERY, IssueQuery};
use rbit_services::issues::LabelUsage;

use crate::views::{
    components::FormError,
    issues::{list_url, safe_color},
    layout::Page,
    repo::RepoHeader,
};

/// One label with its usage and (for writers) its edit and delete targets.
#[derive(Debug, Clone)]
pub struct LabelView {
    pub id: i64,
    pub name: String,
    /// A validated `#rrggbb`.
    pub color: String,
    pub description: String,
    pub open_issues: u64,
    /// The issue list filtered by this label.
    pub issues_url: String,
    pub update_url: String,
    pub delete_url: String,
}

impl LabelView {
    pub fn new(usage: &LabelUsage, base: &str) -> Self {
        let label = &usage.label;
        let issues = format!("{base}/issues");
        Self {
            id: label.id.0,
            name: label.name.clone(),
            color: safe_color(&label.color),
            description: label.description.clone(),
            open_issues: usage.open_issues,
            issues_url: list_url(&issues, &IssueQuery::parse(DEFAULT_QUERY).toggling_label(&label.name)),
            update_url: format!("{base}/labels/{}", label.id.0),
            delete_url: format!("{base}/labels/{}/delete", label.id.0),
        }
    }
}

/// The values of the "New label" form.
#[derive(Debug, Clone)]
pub struct LabelForm {
    pub name: String,
    pub color: String,
    pub description: String,
}

impl Default for LabelForm {
    fn default() -> Self {
        Self { name: String::new(), color: "#888888".to_owned(), description: String::new() }
    }
}

/// `/{owner}/{repo}/labels`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "issues/labels.html")]
pub struct LabelsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub labels: Vec<LabelView>,
    pub can_write: bool,
    pub create_url: String,
    pub defaults_url: String,
    pub issues_url: String,
    pub milestones_url: String,
    pub error: FormError,
    pub form: LabelForm,
}

#[cfg(test)]
mod tests {
    use rbit_core::{LabelId, RepoId, Timestamp};
    use rbit_services::issues::Label;

    use super::*;

    #[test]
    fn labels_render_with_svg_colours_and_writer_controls() {
        let usage = LabelUsage {
            label: Label {
                id: LabelId(4),
                repo_id: RepoId(1),
                name: "good first issue".into(),
                color: "#7057ff".into(),
                description: "Easy <b>start</b>".into(),
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
            },
            open_issues: 2,
        };
        let view = LabelView::new(&usage, "/alice/demo");
        assert_eq!(view.update_url, "/alice/demo/labels/4");
        assert_eq!(view.issues_url, "/alice/demo/issues?q=is%3Aopen%20is%3Aissue%20label%3A%22good%20first%20issue%22");
        let page = |can_write| LabelsPage {
            page: Page {
                updates: rbit_services::updates::UpdateStatus::default(),
                site_name: "rbit".into(),
                plugin_revision: 0,
                branding: rbit_core::config::BrandingConfig::default(),
                title: None,
                viewer: None,
                path: "/alice/demo/labels".into(),
                has_unread_notifications: false,
                theme: crate::themes::Theme::System,
            },
            header: RepoHeader::default(),
            labels: vec![view.clone()],
            can_write,
            create_url: "/alice/demo/labels".into(),
            defaults_url: "/alice/demo/labels/defaults".into(),
            issues_url: "/alice/demo/issues".into(),
            milestones_url: "/alice/demo/milestones".into(),
            error: FormError::default(),
            form: LabelForm::default(),
        };
        let html = page(true).render().unwrap();
        assert!(html.contains(r##"fill="#7057ff""##) && html.contains("New label") && html.contains("/alice/demo/labels/4/delete"));
        assert!(!html.contains("<b>start"), "descriptions are escaped");
        assert!(!html.contains("style="));
        let html = page(false).render().unwrap();
        assert!(!html.contains("New label") && !html.contains("/delete"));
    }
}
