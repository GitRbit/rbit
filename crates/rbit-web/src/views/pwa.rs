//! The offline page used by the progressive web app.

use askama::Template;
use askama_web::WebTemplate;

use crate::views::layout::Page;

/// Cached by the service worker and shown when a navigation fails offline.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "offline.html")]
pub struct OfflinePage {
    pub page: Page,
}
