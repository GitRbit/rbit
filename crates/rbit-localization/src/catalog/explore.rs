//! The repository listings and Explore: titles, controls, counts, empty states and paging.
//! Static English labels borrow compiled literals; translations live in
//! `translations/<locale>/explore.rs`.
use super::display::{PAGE_OF_SEPARATOR, PAGE_PREFIX, STARS_SUFFIX, SUBTITLE_PREFIX, SUBTITLE_SUFFIX};
use super::fallback::{MessageFamily, MessageWriter};
use super::{Catalog, LocalizedText, RepositoryVisibility};
use crate::Locale;

/// Stable typed message contract. Each variant has a complete English entry; a locale
/// without a translation falls back to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Message<'a> {
    RepositoriesTitle,
    RepositoryCount { count: u64 },
    OwnerRepositories { owner: &'a str },
    Visibility { visibility: RepositoryVisibility },
    EmptyRepositories,
    SearchRepositories,
    SearchLabel,
    NextPage,
    PreviousPage,
    PageNumber { page: u64 },
    RepositoryLinkLabel { owner: &'a str, name: &'a str },
    ExploreTitle,
    ExploreSubtitle { site: &'a str },
    ExploreSearchPlaceholder,
    SortBy,
    SortUpdated,
    SortName,
    SortNewest,
    ArchiveFilterLabel,
    ArchiveAll,
    ArchiveActive,
    Archived,
    NothingHereYet,
    PublicRepositoriesEmpty,
    NoMatchingRepositories,
    SearchSuggestion,
    CountForQuery { count: u64, query: &'a str },
    PaginationLabel,
    PageOf { page: u32, pages: u32 },
    Stars { count: i64 },
}

impl Message<'_> {
    /// Catalog identifier, independent of the translated text and arguments.
    pub const fn id(self) -> &'static str {
        match self {
            Self::RepositoriesTitle => "repositories.title",
            Self::RepositoryCount { .. } => "repositories.count",
            Self::OwnerRepositories { .. } => "repositories.owner-title",
            Self::Visibility { .. } => "repositories.visibility",
            Self::EmptyRepositories => "repositories.empty",
            Self::SearchRepositories => "repositories.search-placeholder",
            Self::SearchLabel => "common.search",
            Self::NextPage => "pagination.next",
            Self::PreviousPage => "pagination.previous",
            Self::PageNumber { .. } => "pagination.page-number",
            Self::RepositoryLinkLabel { .. } => "repositories.link-label",
            Self::ExploreTitle => "explore.title",
            Self::ExploreSubtitle { .. } => "explore.subtitle",
            Self::ExploreSearchPlaceholder => "explore.search-placeholder",
            Self::SortBy => "repositories.sort-label",
            Self::SortUpdated => "repositories.sort-updated",
            Self::SortName => "repositories.sort-name",
            Self::SortNewest => "repositories.sort-newest",
            Self::ArchiveFilterLabel => "repositories.archive-filter-label",
            Self::ArchiveAll => "repositories.archive-all",
            Self::ArchiveActive => "repositories.archive-active",
            Self::Archived => "repositories.archived",
            Self::NothingHereYet => "explore.empty-title",
            Self::PublicRepositoriesEmpty => "explore.empty-description",
            Self::NoMatchingRepositories => "explore.no-match-title",
            Self::SearchSuggestion => "explore.no-match-description",
            Self::CountForQuery { .. } => "explore.count-for-query",
            Self::PaginationLabel => "pagination.label",
            Self::PageOf { .. } => "pagination.page-of",
            Self::Stars { .. } => "repositories.stars",
        }
    }

    /// The English text of a fixed label.
    const fn english_label(self) -> Option<&'static str> {
        Some(match self {
            Self::RepositoriesTitle => "Repositories",
            Self::Visibility { visibility } => match visibility {
                RepositoryVisibility::Public => "Public",
                RepositoryVisibility::Private => "Private",
                RepositoryVisibility::Internal => "Internal",
            },
            Self::EmptyRepositories => "No repositories found.",
            Self::SearchRepositories => "Search repositories",
            Self::SearchLabel => "Search",
            Self::NextPage => "Next",
            Self::PreviousPage => "Previous",
            Self::ExploreTitle => "Explore",
            Self::ExploreSearchPlaceholder => "Search by name or description",
            Self::SortBy => "Sort by",
            Self::SortUpdated => "Updated",
            Self::SortName => "Name",
            Self::SortNewest => "Newest",
            Self::ArchiveFilterLabel => "Archived repositories",
            Self::ArchiveAll => "All",
            Self::ArchiveActive => "Active",
            Self::Archived => "Archived",
            Self::NothingHereYet => "Nothing here yet",
            Self::PublicRepositoriesEmpty => "Public repositories will be listed here once they exist.",
            Self::NoMatchingRepositories => "No matching repositories",
            Self::SearchSuggestion => "Try a shorter search or different words.",
            Self::PaginationLabel => "Pagination",
            Self::RepositoryCount { .. }
            | Self::OwnerRepositories { .. }
            | Self::PageNumber { .. }
            | Self::RepositoryLinkLabel { .. }
            | Self::ExploreSubtitle { .. }
            | Self::CountForQuery { .. }
            | Self::PageOf { .. }
            | Self::Stars { .. } => return None,
        })
    }
}

/// A count of repositories: `count` and its English noun.
fn repositories(text: &mut MessageWriter, count: u64) {
    text.number(count);
    text.literal(if count == 1 { " repository" } else { " repositories" });
}

impl MessageFamily for Message<'_> {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::RepositoryCount { count } => repositories(text, count),
            Self::OwnerRepositories { owner } => {
                text.literal("Repositories owned by ");
                text.argument(owner);
            }
            Self::PageNumber { page } => {
                text.literal(PAGE_PREFIX);
                text.number(page);
            }
            Self::RepositoryLinkLabel { owner, name } => {
                text.literal("Open repository ");
                text.argument(owner);
                text.literal("/");
                text.argument(name);
            }
            Self::ExploreSubtitle { site } => {
                text.literal(SUBTITLE_PREFIX);
                text.argument(site);
                text.literal(SUBTITLE_SUFFIX);
            }
            Self::CountForQuery { count, query } => {
                repositories(text, count);
                if !query.is_empty() {
                    text.literal(" for “");
                    text.argument(query);
                    text.literal("”");
                }
            }
            Self::PageOf { page, pages } => {
                text.literal(PAGE_PREFIX);
                text.number(page.into());
                text.literal(PAGE_OF_SEPARATOR);
                text.number(pages.into());
            }
            Self::Stars { count } => {
                signed(text, count);
                text.literal(STARS_SUFFIX);
            }
            _ => text.literal(self.english_label().unwrap_or_default()),
        }
    }

    fn english_literal(self) -> Option<&'static str> {
        self.english_label()
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// A signed count, such as a star total: a minus sign, then its magnitude.
pub(crate) fn signed(text: &mut MessageWriter, count: i64) {
    if count < 0 {
        text.literal("-");
    }
    text.number(count.unsigned_abs());
}

impl Catalog {
    /// Formats a typed message into plain text, or its English text where this locale has
    /// no translation. Arguments are never interpreted as catalog syntax, markup or
    /// formatting instructions. Templates escape once.
    pub fn text(self, message: Message<'_>) -> LocalizedText {
        self.localized(message).into_text()
    }
}
