//! Shared wiki view models: localized text, URLs and the chrome every wiki page shows
//! (repository header, page list, write controls). Page templates live in
//! [`super::wiki_pages`]; templates name messages as `WikiMessage::…` through
//! [`WikiText`] and contain no English literals.
use rbit_core::Timestamp;
use rbit_git::diff::{Hunk, TextChanges};
use rbit_localization::{Catalog, Locale, Localized, WikiMessage};
use rbit_services::wikis::errors::WikiError;
use rbit_wikis::{WikiContentError, WikiNameError, WikiPageId, WikiPath, change::WikiChange, listing::WikiPageEntry};

use crate::{
    urls::encode_path,
    views::commit::{DiffLineView, width_classes},
};

/// Wiki messages in the request's presentation locale.
#[derive(Debug, Clone, Copy)]
pub struct WikiText(Catalog);

impl WikiText {
    pub const fn new(locale: Locale) -> Self {
        Self(Catalog::new(locale))
    }

    pub fn text(&self, message: WikiMessage<'_>) -> Localized {
        self.0.localized(message)
    }

    /// The history label of a change kind (`None`: not recorded).
    pub fn change(&self, change: Option<WikiChange>) -> Localized {
        self.text(match change {
            Some(WikiChange::Create) => WikiMessage::Created,
            Some(WikiChange::Rename) => WikiMessage::Renamed,
            Some(WikiChange::Delete) => WikiMessage::Deleted,
            Some(WikiChange::Restore) => WikiMessage::Restored,
            Some(WikiChange::Edit) | None => WikiMessage::Edited,
        })
    }

    /// The message explaining a refused form, or `None` for errors that are not about
    /// the submitted fields.
    pub fn form_error(&self, error: &WikiError) -> Option<Localized> {
        let message = match error {
            WikiError::InvalidTitle(WikiNameError::Empty) => WikiMessage::TitleEmpty,
            WikiError::InvalidTitle(WikiNameError::TooLong | WikiNameError::RawLimit) => WikiMessage::TitleTooLong,
            WikiError::InvalidTitle(_) => WikiMessage::TitleInvalid,
            WikiError::InvalidPath(WikiNameError::Empty) => WikiMessage::PathEmpty,
            WikiError::InvalidPath(WikiNameError::TooLong | WikiNameError::RawLimit) => WikiMessage::PathTooLong,
            WikiError::InvalidPath(WikiNameError::TooDeep) => WikiMessage::PathTooDeep,
            WikiError::InvalidPath(WikiNameError::ReservedPath) => WikiMessage::PathReserved,
            WikiError::InvalidPath(_) => WikiMessage::PathInvalid,
            WikiError::InvalidContent(WikiContentError::TooLarge) => WikiMessage::ContentTooLarge,
            WikiError::InvalidContent(WikiContentError::Nul) => WikiMessage::ContentInvalid,
            WikiError::Conflict => WikiMessage::Conflict,
            WikiError::PathTaken => WikiMessage::PathTaken,
            WikiError::Unchanged => WikiMessage::Unchanged,
            _ => return None,
        };
        Some(self.text(message))
    }
}

/// Every URL of one repository's wiki. Page paths are encoded segment by segment.
#[derive(Debug, Clone)]
pub struct WikiUrls {
    pub home: String,
}

impl WikiUrls {
    pub fn new(repository_home: &str) -> Self {
        Self { home: format!("{repository_home}/wiki") }
    }

    pub fn page(&self, path: &WikiPath) -> String {
        format!("{}/{}", self.home, encode_path(path.as_str()))
    }

    pub fn pages(&self) -> String {
        format!("{}/-/pages", self.home)
    }

    pub fn search(&self) -> String {
        format!("{}/-/search", self.home)
    }

    pub fn new_page(&self) -> String {
        format!("{}/-/new", self.home)
    }

    pub fn settings(&self) -> String {
        format!("{}/-/settings", self.home)
    }

    /// `W/-/deleted`: deleted pages, with their restore forms.
    pub fn deleted(&self) -> String {
        format!("{}/-/deleted", self.home)
    }

    /// `W/-/page/{id}/{action}`: `edit`, `history`, `delete`, `undelete` or `compare`.
    pub fn action(&self, page: WikiPageId, action: &str) -> String {
        format!("{}/-/page/{}/{action}", self.home, page.get())
    }

    pub fn revision(&self, page: WikiPageId, generation: i64) -> String {
        format!("{}/-/page/{}/revisions/{generation}", self.home, page.get())
    }

    pub fn compare(&self, page: WikiPageId, base: i64, head: i64) -> String {
        format!("{}/-/page/{}/compare?base={base}&head={head}", self.home, page.get())
    }
}

/// One page in the page list.
#[derive(Debug, Clone)]
pub struct WikiEntryView {
    pub title: String,
    pub path: String,
    pub url: String,
    pub is_current: bool,
}

impl WikiEntryView {
    pub fn new(entry: &WikiPageEntry, urls: &WikiUrls, current: Option<WikiPageId>) -> Self {
        Self {
            title: entry.title.to_string(),
            path: entry.path.to_string(),
            url: urls.page(&entry.path),
            is_current: current == Some(entry.id),
        }
    }
}

/// What every wiki page shows around its content.
#[derive(Debug, Clone)]
pub struct WikiChrome {
    pub t: WikiText,
    pub urls: WikiUrls,
    pub sidebar: Vec<WikiEntryView>,
    pub more_pages: bool,
    /// Write access to a repository that is not archived: show change controls.
    pub can_write: bool,
    /// Repository administrators may turn the wiki off.
    pub can_admin: bool,
}

/// `YYYY-MM-DD` (UTC) for display and the full RFC 3339 value for `datetime`.
pub fn dates(at: Timestamp) -> (String, String) {
    let iso = at.iso8601();
    (iso.get(..10).unwrap_or(&iso).to_owned(), iso)
}

/// One breadcrumb: the page path's ancestors are not pages themselves, so only the
/// last crumb links (to the page).
#[derive(Debug, Clone)]
pub struct Crumb {
    pub name: String,
    pub is_last: bool,
}

pub fn crumbs(path: &WikiPath) -> Vec<Crumb> {
    let count = path.as_str().split('/').count();
    path.as_str().split('/').enumerate().map(|(index, name)| Crumb { name: name.to_owned(), is_last: index + 1 == count }).collect()
}

/// Rows of a page diff, shaped like commit diffs (`repo/diff_rows.html`).
#[derive(Debug, Clone)]
pub struct WikiDiffView {
    pub rows: Vec<DiffLineView>,
    pub width_classes: String,
    pub additions: u64,
    pub deletions: u64,
}

impl WikiDiffView {
    pub fn new(changes: TextChanges) -> Self {
        let width_classes = width_classes(&changes.hunks);
        let mut rows = Vec::with_capacity(changes.line_count() + changes.hunks.len());
        for Hunk { header, lines } in changes.hunks {
            rows.push(DiffLineView::hunk(header));
            for line in lines {
                let eof = line.missing_newline;
                rows.push(DiffLineView::owned_line(line));
                if eof {
                    rows.push(DiffLineView::eof());
                }
            }
        }
        Self {
            rows,
            width_classes,
            additions: u64::try_from(changes.additions).unwrap_or(u64::MAX),
            deletions: u64::try_from(changes.deletions).unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_encode_each_path_segment_and_keep_actions_under_the_reserved_namespace() {
        let urls = WikiUrls::new("/alice/demo");
        let path: WikiPath = "Guides/Café_1".parse().unwrap();
        assert_eq!(urls.page(&path), "/alice/demo/wiki/Guides/Caf%C3%A9_1");
        let page = WikiPageId::new(7).unwrap();
        assert_eq!(urls.action(page, "edit"), "/alice/demo/wiki/-/page/7/edit");
        assert_eq!(urls.compare(page, 2, 5), "/alice/demo/wiki/-/page/7/compare?base=2&head=5");
        let crumbs = crumbs(&path);
        assert_eq!((crumbs.len(), crumbs[1].is_last, crumbs[0].name.as_str()), (2, true, "Guides"));
    }

    #[test]
    fn field_errors_map_to_messages_and_other_errors_do_not() {
        let text = WikiText::new(Locale::English);
        assert_eq!(text.form_error(&WikiError::InvalidTitle(WikiNameError::Empty)).unwrap().as_str(), "Enter a title.");
        assert_eq!(text.form_error(&WikiError::PathTaken).unwrap().as_str(), "Another page already uses this path.");
        assert!(text.form_error(&WikiError::Forbidden).is_none());
        assert_eq!(text.change(Some(WikiChange::Restore)).as_str(), "Restored");
    }
}
