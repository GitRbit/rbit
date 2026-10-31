//! Host-owned plugin administration, including builds without optional modules.

use askama::Template;
use askama_web::WebTemplate;

use super::{
    admin::{AdminLink, AdminSection},
    layout::Page,
};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/plugins.html")]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct PluginsPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub available: bool,
    pub active: bool,
    pub retry_disable: bool,
    pub busy: bool,
    pub status: String,
    pub generation: Option<u64>,
    pub error: Option<String>,
}

impl PluginsPage {
    pub fn new(page: Page, available: bool, active: bool, status: String, generation: Option<u64>) -> Self {
        Self {
            page: page.titled("Plugins"),
            admin_nav: AdminSection::Plugins.nav(),
            available,
            active,
            retry_disable: false,
            busy: false,
            status,
            generation,
            error: None,
        }
    }
}
