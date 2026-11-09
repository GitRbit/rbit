//! Wiki page templates (`templates/wiki/*.html`). Shared parts are in [`super::wiki`].
//! Templates name messages as `WikiMessage::…`, resolved against this module's imports.
use askama::Template;
use askama_web::WebTemplate;
use rbit_localization::{Localized, WikiMessage};

use crate::views::{
    layout::Page,
    repo::RepoHeader,
    wiki::{Crumb, WikiChrome, WikiDiffView, WikiEntryView},
};

/// Shown above a page read at an older revision.
#[derive(Debug, Clone)]
pub struct WikiRevisionNote {
    pub label: Localized,
    /// The current page, or the deleted pages when the page is deleted.
    pub current_url: String,
    /// `None` for a deleted page: it is restored as a whole from the deleted pages.
    pub restore_url: Option<String>,
    /// The whole-wiki generation the restore form was read at.
    pub base: i64,
    pub compare_url: Option<String>,
}

/// `W` (the home page), `W/{path}` and `W/-/page/{id}/revisions/{n}`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/page.html")]
pub struct WikiPageTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub title: String,
    pub crumbs: Vec<Crumb>,
    /// Sanitized by the shared Markdown renderer.
    pub html: String,
    pub updated: Localized,
    pub updated_iso: String,
    pub edit_url: String,
    pub history_url: String,
    pub delete_url: String,
    pub revision: Option<WikiRevisionNote>,
}

/// A link that creates a page.
#[derive(Debug, Clone)]
pub struct WikiCreateLink {
    pub url: String,
    pub label: Localized,
}

/// An empty or turned-off wiki, or a path without a page.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/state.html")]
pub struct WikiStateTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub message: Localized,
    pub about: Option<Localized>,
    pub create: Option<WikiCreateLink>,
    /// The settings form, offered to administrators of a turned-off wiki.
    pub enable_url: Option<String>,
}

/// `W/-/pages` and `W/-/search`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/pages.html")]
pub struct WikiIndexTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub heading: Localized,
    pub entries: Vec<WikiEntryView>,
    pub empty: Localized,
    pub next_url: Option<String>,
    pub show_settings: bool,
}

/// One revision in a page's history.
#[derive(Debug, Clone)]
pub struct WikiHistoryRow {
    pub label: Localized,
    /// `None` for a deletion, which has no content to show.
    pub url: Option<String>,
    /// "Renamed from …" when this revision moved the page.
    pub renamed_from: Option<Localized>,
    pub title: String,
    pub change: Localized,
    pub author: Option<String>,
    pub date: Localized,
    pub iso: String,
    pub compare_url: Option<String>,
}

/// `W/-/page/{id}/history`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/history.html")]
pub struct WikiHistoryTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub heading: Localized,
    /// The current page, or the deleted pages when the page is deleted.
    pub page_url: String,
    /// Shown for a deleted page.
    pub deleted_notice: Option<Localized>,
    pub entries: Vec<WikiHistoryRow>,
    pub older_url: Option<String>,
}

/// `W/-/page/{id}/compare`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/compare.html")]
pub struct WikiCompareTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub title: String,
    pub heading: Localized,
    pub summary: Localized,
    pub changes: WikiDiffView,
    pub history_url: String,
}

/// Where a writer goes to resolve a conflict.
#[derive(Debug, Clone)]
pub struct WikiConflictLinks {
    pub current_url: String,
    pub compare_url: Option<String>,
}

/// `W/-/new` and `W/-/page/{id}/edit`, re-rendered with the submitted text on refusal.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/edit.html")]
pub struct WikiEditTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub heading: Localized,
    pub action_url: String,
    pub cancel_url: String,
    pub title: String,
    pub path: String,
    pub content: String,
    /// The whole-wiki generation an edit form was read at (`None` when creating).
    pub base: Option<i64>,
    pub error: Option<Localized>,
    pub conflict: Option<WikiConflictLinks>,
    /// Sanitized preview HTML, when the preview button was used.
    pub preview: Option<String>,
}

/// `W/-/page/{id}/delete`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/delete.html")]
pub struct WikiDeleteTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub heading: Localized,
    pub action_url: String,
    pub cancel_url: String,
    pub base: i64,
}

/// One deleted page with its restore form.
#[derive(Debug, Clone)]
pub struct WikiDeletedRow {
    pub title: String,
    pub path: String,
    pub history_url: String,
    pub restore_url: String,
    /// The generation that deleted the page: restoring after another change conflicts.
    pub base: i64,
    pub deleted_by: Option<String>,
    pub date: Localized,
    pub iso: String,
}

/// `W/-/deleted`: deleted pages, most recently deleted first.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "wiki/deleted.html")]
pub struct WikiDeletedTemplate {
    pub page: Page,
    pub header: RepoHeader,
    pub wiki: WikiChrome,
    pub entries: Vec<WikiDeletedRow>,
    /// Why a restore was refused.
    pub error: Option<Localized>,
    pub older_url: Option<String>,
}
