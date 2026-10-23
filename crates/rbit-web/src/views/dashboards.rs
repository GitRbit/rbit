//! Personal dashboard view models.
use super::{components::Pagination, issues::IssueRow, layout::Page};
use askama::Template;
use askama_web::WebTemplate;

#[derive(Debug)]
pub struct DashboardTab {
    pub label: &'static str,
    pub url: String,
    pub active: bool,
}
#[derive(Debug)]
pub struct DashboardRow {
    pub repository: String,
    pub row: IssueRow,
}
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/threads.html")]
pub struct DashboardPage {
    pub page: Page,
    pub title: &'static str,
    pub base: &'static str,
    pub view: &'static str,
    pub query: String,
    pub tabs: Vec<DashboardTab>,
    pub rows: Vec<DashboardRow>,
    pub hints: Vec<String>,
    pub open: u64,
    pub closed: u64,
    pub open_url: String,
    pub closed_url: String,
    pub pagination: Pagination,
}
