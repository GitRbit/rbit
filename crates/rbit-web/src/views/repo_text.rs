//! Repository page text chosen in handlers, such as document titles, in the page's
//! language. Untranslated messages fall back to English (`rbit_localization::Localized`).
use rbit_localization::{Catalog, Localized, MessageFamily};

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

/// Titles `page` with `message` alone, in the page's language.
pub fn titled_as<M: MessageFamily>(page: Page, message: M) -> Page {
    let title = text(&page, message);
    page.titled(title.to_string())
}
