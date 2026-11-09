//! Repository wiki chrome. English is the only catalog so far: other locales fall back
//! to it through the shared [`MessageFamily`] mechanism, which marks that text English.
//! Page titles, paths and content are user text and are never translated.
use super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiMessage<'a> {
    Tab,
    Pages,
    AllPages,
    PagesLabel,
    BreadcrumbLabel,
    SearchPages,
    Search,
    NewPage,
    EditPage,
    Edit,
    History,
    Delete,
    LastEdited,
    Revision { generation: i64 },
    RevisionColumn,
    Author,
    Date,
    UnknownAuthor,
    Created,
    Edited,
    Renamed,
    Deleted,
    Restored,
    View,
    CompareWithPrevious,
    Restore,
    OlderRevisions,
    OldRevisionNotice,
    ViewCurrent,
    HistoryOf { title: &'a str },
    Comparing { base: i64, head: i64 },
    Changes { additions: u64, deletions: u64 },
    NoDifferences,
    Title,
    Path,
    PathHint,
    Content,
    Markdown,
    Save,
    Preview,
    NothingToPreview,
    Cancel,
    TitleEmpty,
    TitleTooLong,
    TitleInvalid,
    PathEmpty,
    PathTooLong,
    PathTooDeep,
    PathInvalid,
    PathReserved,
    ContentTooLarge,
    ContentInvalid,
    Conflict,
    CompareChanges,
    PathTaken,
    Unchanged,
    DeleteTitle { title: &'a str },
    DeleteWarning,
    DeletePage,
    Disabled,
    About,
    TurnOn,
    TurnOff,
    Empty,
    CreateFirst,
    Missing,
    CreatePath { path: &'a str },
    SearchCount { count: u64, query: &'a str },
    NoResults { query: &'a str },
    MorePages,
}

impl WikiMessage<'_> {
    const fn key(self) -> &'static str {
        match self {
            Self::Tab => "wiki.tab",
            Self::Pages => "wiki.tree.title",
            Self::AllPages => "wiki.tree.all",
            Self::PagesLabel => "wiki.a11y.pages",
            Self::BreadcrumbLabel => "wiki.a11y.breadcrumb",
            Self::SearchPages => "wiki.search.placeholder",
            Self::Search => "wiki.search.submit",
            Self::NewPage => "wiki.page.new",
            Self::EditPage => "wiki.edit.title",
            Self::Edit => "wiki.page.edit",
            Self::History => "wiki.page.history",
            Self::Delete => "wiki.page.delete",
            Self::LastEdited => "wiki.page.last-edited",
            Self::Revision { .. } => "wiki.history.revision",
            Self::RevisionColumn => "wiki.history.revision-column",
            Self::Author => "wiki.history.author",
            Self::Date => "wiki.history.date",
            Self::UnknownAuthor => "wiki.history.unknown-author",
            Self::Created => "wiki.history.created",
            Self::Edited => "wiki.history.edited",
            Self::Renamed => "wiki.history.renamed",
            Self::Deleted => "wiki.history.deleted",
            Self::Restored => "wiki.history.restored",
            Self::View => "wiki.history.view",
            Self::CompareWithPrevious => "wiki.history.compare-previous",
            Self::Restore => "wiki.history.restore",
            Self::OlderRevisions => "wiki.history.older",
            Self::OldRevisionNotice => "wiki.history.old-notice",
            Self::ViewCurrent => "wiki.history.view-current",
            Self::HistoryOf { .. } => "wiki.history.title",
            Self::Comparing { .. } => "wiki.compare.title",
            Self::Changes { .. } => "wiki.compare.changes",
            Self::NoDifferences => "wiki.compare.none",
            Self::Title => "wiki.edit.field-title",
            Self::Path => "wiki.edit.field-path",
            Self::PathHint => "wiki.edit.path-hint",
            Self::Content => "wiki.edit.field-content",
            Self::Markdown => "wiki.edit.markdown",
            Self::Save => "wiki.edit.save",
            Self::Preview => "wiki.edit.preview",
            Self::NothingToPreview => "wiki.edit.preview-empty",
            Self::Cancel => "wiki.edit.cancel",
            Self::TitleEmpty => "wiki.validation.title-empty",
            Self::TitleTooLong => "wiki.validation.title-long",
            Self::TitleInvalid => "wiki.validation.title-invalid",
            Self::PathEmpty => "wiki.validation.path-empty",
            Self::PathTooLong => "wiki.validation.path-long",
            Self::PathTooDeep => "wiki.validation.path-deep",
            Self::PathInvalid => "wiki.validation.path-invalid",
            Self::PathReserved => "wiki.validation.path-reserved",
            Self::ContentTooLarge => "wiki.validation.content-large",
            Self::ContentInvalid => "wiki.validation.content-invalid",
            Self::Conflict => "wiki.conflict.changed",
            Self::CompareChanges => "wiki.conflict.compare",
            Self::PathTaken => "wiki.conflict.path-taken",
            Self::Unchanged => "wiki.validation.unchanged",
            Self::DeleteTitle { .. } => "wiki.delete.title",
            Self::DeleteWarning => "wiki.delete.warning",
            Self::DeletePage => "wiki.delete.confirm",
            Self::Disabled => "wiki.settings.disabled",
            Self::About => "wiki.settings.about",
            Self::TurnOn => "wiki.settings.enable",
            Self::TurnOff => "wiki.settings.disable",
            Self::Empty => "wiki.page.empty",
            Self::CreateFirst => "wiki.page.create-first",
            Self::Missing => "wiki.page.missing",
            Self::CreatePath { .. } => "wiki.page.create-path",
            Self::SearchCount { .. } => "wiki.search.count",
            Self::NoResults { .. } => "wiki.search.none",
            Self::MorePages => "wiki.tree.more",
        }
    }

    const fn literal(self) -> Option<&'static str> {
        Some(match self {
            Self::Tab => "Wiki",
            Self::Pages => "Pages",
            Self::AllPages => "All pages",
            Self::PagesLabel => "Wiki pages",
            Self::BreadcrumbLabel => "Breadcrumb",
            Self::SearchPages => "Search pages",
            Self::Search => "Search",
            Self::NewPage => "New page",
            Self::EditPage => "Edit page",
            Self::Edit => "Edit",
            Self::History => "History",
            Self::Delete => "Delete",
            Self::LastEdited => "Last edited",
            Self::RevisionColumn => "Revision",
            Self::Author => "Author",
            Self::Date => "Date",
            Self::UnknownAuthor => "Unknown author",
            Self::Created => "Created",
            Self::Edited => "Edited",
            Self::Renamed => "Renamed",
            Self::Deleted => "Deleted",
            Self::Restored => "Restored",
            Self::View => "View",
            Self::CompareWithPrevious => "Compare with previous",
            Self::Restore => "Restore this revision",
            Self::OlderRevisions => "Older revisions",
            Self::OldRevisionNotice => "You are viewing an older revision of this page.",
            Self::ViewCurrent => "View the current page",
            Self::NoDifferences => "These revisions have the same content.",
            Self::Title => "Title",
            Self::Path => "Path",
            Self::PathHint => "Letters, digits, - _ and . separated by /. Leave it empty to derive it from the title.",
            Self::Content => "Content",
            Self::Markdown => "Markdown",
            Self::Save => "Save page",
            Self::Preview => "Preview",
            Self::NothingToPreview => "Nothing to preview.",
            Self::Cancel => "Cancel",
            Self::TitleEmpty => "Enter a title.",
            Self::TitleTooLong => "The title is too long.",
            Self::TitleInvalid => "The title contains characters that are not allowed.",
            Self::PathEmpty => "Enter a path, or a title it can be derived from.",
            Self::PathTooLong => "The path or one of its parts is too long.",
            Self::PathTooDeep => "The path has too many parts.",
            Self::PathInvalid => "Each part of the path must start with a letter or digit and use only letters, digits, - _ and .",
            Self::PathReserved => "The path uses a reserved name.",
            Self::ContentTooLarge => "The content is too large.",
            Self::ContentInvalid => "The content contains characters that are not allowed.",
            Self::Conflict => "This page changed after you started. Your text is kept below; review the current page before saving again.",
            Self::CompareChanges => "Compare the changes",
            Self::PathTaken => "Another page already uses this path.",
            Self::Unchanged => "Nothing changed.",
            Self::DeleteWarning => "The page and its history are removed from the wiki. This cannot be undone here.",
            Self::DeletePage => "Delete page",
            Self::Disabled => "The wiki is turned off for this repository.",
            Self::About => "A wiki holds documentation pages with their own history, separate from the code.",
            Self::TurnOn => "Turn on the wiki",
            Self::TurnOff => "Turn off the wiki",
            Self::Empty => "This wiki has no pages yet.",
            Self::CreateFirst => "Create the first page",
            Self::Missing => "No page has this path.",
            Self::MorePages => "More pages",
            Self::Revision { .. }
            | Self::HistoryOf { .. }
            | Self::Comparing { .. }
            | Self::Changes { .. }
            | Self::DeleteTitle { .. }
            | Self::CreatePath { .. }
            | Self::SearchCount { .. }
            | Self::NoResults { .. } => return None,
        })
    }
}

impl MessageFamily for WikiMessage<'_> {
    fn id(self) -> &'static str {
        self.key()
    }

    fn english(self, text: &mut MessageWriter) {
        if let Some(literal) = self.literal() {
            text.literal(literal);
            return;
        }
        let number = |value: i64| u64::try_from(value).unwrap_or_default();
        match self {
            Self::Revision { generation } => {
                text.literal("Revision ");
                text.number(number(generation));
            }
            Self::HistoryOf { title } => {
                text.literal("History of ");
                text.argument(title);
            }
            Self::Comparing { base, head } => {
                text.literal("Changes from revision ");
                text.number(number(base));
                text.literal(" to revision ");
                text.number(number(head));
            }
            Self::Changes { additions, deletions } => {
                text.number(additions);
                text.literal(if additions == 1 { " addition, " } else { " additions, " });
                text.number(deletions);
                text.literal(if deletions == 1 { " deletion" } else { " deletions" });
            }
            Self::DeleteTitle { title } => {
                text.literal("Delete ");
                text.argument(title);
                text.literal("?");
            }
            Self::CreatePath { path } => {
                text.literal("Create ");
                text.argument(path);
            }
            Self::SearchCount { count, query } => {
                text.number(count);
                text.literal(if count == 1 { " page matches “" } else { " pages match “" });
                text.argument(query);
                text.literal("”");
            }
            Self::NoResults { query } => {
                text.literal("No pages match “");
                text.argument(query);
                text.literal("”.");
            }
            _ => {}
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Catalog, Locale};

    #[test]
    fn wiki_messages_have_ids_keep_user_text_plain_and_translate_to_spanish() {
        let attack = "<script>alert(1)</script>";
        for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
            let catalog = Catalog::new(locale);
            let title = catalog.localized(WikiMessage::HistoryOf { title: attack });
            assert!(title.as_str().contains(attack));
            if locale == Locale::PseudoRtl {
                assert!(title.as_str().contains("\u{2068}<script>alert(1)</script>\u{2069}"));
            }
            assert!(!title.fallback());
        }
        let english = Catalog::new(Locale::English);
        assert_eq!(english.localized(WikiMessage::Tab).as_str(), "Wiki");
        assert_eq!(english.localized(WikiMessage::Changes { additions: 1, deletions: 2 }).as_str(), "1 addition, 2 deletions");
        assert_eq!(english.localized(WikiMessage::SearchCount { count: 1, query: "a" }).as_str(), "1 page matches “a”");
        assert_eq!(english.localized(WikiMessage::Revision { generation: 3 }).as_str(), "Revision 3");
        assert_eq!(Catalog::new(Locale::Spanish).localized(WikiMessage::Save).as_str(), "Guardar página");
        assert!(!Catalog::new(Locale::German).localized(WikiMessage::Save).fallback());
        assert_eq!(WikiMessage::Revision { generation: 3 }.id(), "wiki.history.revision");
    }
}
