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
    pub i18n_available: bool,
    pub i18n_active: bool,
    pub i18n_status: String,
    pub i18n_generation: Option<u64>,
    pub i18n_retry_disable: bool,
    pub i18n_busy: bool,
    pub i18n_problem: bool,
    pub packs: Vec<LanguagePack>,
}

impl PluginsPage {
    pub fn new(page: Page, available: bool, active: bool, status: String, generation: Option<u64>) -> Self {
        Self {
            page: page.titled(crate::i18n::text("core", "plugin-plugins")),
            admin_nav: AdminSection::Plugins.nav(),
            available,
            active,
            retry_disable: false,
            busy: false,
            status,
            generation,
            error: None,
            i18n_available: false,
            i18n_active: false,
            i18n_status: String::new(),
            i18n_generation: None,
            i18n_retry_disable: false,
            i18n_busy: false,
            i18n_problem: false,
            packs: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct LanguagePack {
    pub id: String,
    pub locale: String,
    pub version: String,
    pub message_count: usize,
    pub message_total: usize,
}
