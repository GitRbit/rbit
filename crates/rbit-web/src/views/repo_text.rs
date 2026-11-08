//! Repository page text chosen in handlers, such as document titles, in the page's
//! language. Untranslated messages fall back to English (`rbit_localization::Localized`).
use rbit_localization::{Catalog, Localized, MessageFamily, RepoHistoryLabel};

use super::layout::Page;

/// `message` in the page's language, or its English fallback.
pub fn text<M: MessageFamily>(page: &Page, message: M) -> Localized {
    Catalog::new(page.locale).localized(message)
}

/// Titles `page` as `Label · owner/repo`, the label in the page's language.
pub fn titled<M: MessageFamily>(page: Page, label: M, repo: &str) -> Page {
    let label = text(&page, label);
    page.titled(format!("{label} · {repo}"))
}

/// The change-status label for a diff status-mark key (`added`, `deleted`, `modified`,
/// `renamed`, `copied`); anything else reads as modified.
pub fn change_status(key: &str) -> RepoHistoryLabel {
    match key {
        "added" => RepoHistoryLabel::StatusAdded,
        "deleted" => RepoHistoryLabel::StatusDeleted,
        "renamed" => RepoHistoryLabel::StatusRenamed,
        "copied" => RepoHistoryLabel::StatusCopied,
        _ => RepoHistoryLabel::StatusModified,
    }
}

/// Titles `page` with `message` alone, in the page's language.
pub fn titled_as<M: MessageFamily>(page: Page, message: M) -> Page {
    let title = text(&page, message);
    page.titled(title.to_string())
}
