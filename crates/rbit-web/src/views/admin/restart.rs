//! Confirmation and reconnect pages for an administrator-requested server restart.
use askama::Template;
use askama_web::WebTemplate;

use super::{AdminLink, AdminSection};
use crate::views::layout::Page;

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/restart.html")]
pub struct RestartPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub restarting: bool,
    pub available: bool,
}

impl RestartPage {
    pub fn new(page: Page, restarting: bool, available: bool) -> Self {
        Self { page: page.titled("Restart server · Administration"), admin_nav: AdminSection::Dashboard.nav(), restarting, available }
    }
}
