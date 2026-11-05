//! Compiled messages for the first repository-listing slice; no runtime parsing.

use std::{borrow::Cow, fmt, fmt::Write};

use crate::{Locale, pseudo};

/// Typed visibility selector. Adapters map domain values without localized strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryVisibility {
    Public,
    Private,
    Internal,
}

/// Stable typed message contract. Each variant has a complete compiled catalog entry.
/// Counts use English cardinal rules only; pseudo catalogs deliberately share them.
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
}

/// Plain Unicode text, never an HTML fragment. Use ordinary Askama interpolation;
/// `safe` or unescaped `innerHTML` would violate this type's presentation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText(Cow<'static, str>);

impl LocalizedText {
    /// Borrows plain text for templates, text mail, accessible labels or CLI output.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LocalizedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A copyable catalog handle; all entries are compiled into the executable.
#[derive(Clone, Copy, Debug)]
pub struct Catalog {
    locale: Locale,
}

impl Catalog {
    /// Selects one complete catalog without allocation or parsing.
    pub const fn new(locale: Locale) -> Self {
        Self { locale }
    }

    /// Locale used for text, document language/direction and cache dimensions.
    pub const fn locale(self) -> Locale {
        self.locale
    }

    /// Formats a typed message into plain text. Arguments are never interpreted as
    /// catalog syntax, markup or formatting instructions. Templates escape once.
    pub fn text(self, message: Message<'_>) -> LocalizedText {
        let mut output = String::new();
        match message {
            Message::RepositoriesTitle => return self.literal_text("Repositories"),
            Message::RepositoryCount { count } => {
                let _ = write!(output, "{count}");
                self.literal(&mut output, if count == 1 { " repository" } else { " repositories" });
            }
            Message::OwnerRepositories { owner } => {
                self.literal(&mut output, "Repositories owned by ");
                self.argument(&mut output, owner);
            }
            Message::Visibility { visibility } => {
                return self.literal_text(match visibility {
                    RepositoryVisibility::Public => "Public",
                    RepositoryVisibility::Private => "Private",
                    RepositoryVisibility::Internal => "Internal",
                });
            }
            Message::EmptyRepositories => return self.literal_text("No repositories found."),
            Message::SearchRepositories => return self.literal_text("Search repositories"),
            Message::SearchLabel => return self.literal_text("Search"),
            Message::NextPage => return self.literal_text("Next"),
            Message::PreviousPage => return self.literal_text("Previous"),
            Message::PageNumber { page } => {
                self.literal(&mut output, "Page ");
                let _ = write!(output, "{page}");
            }
            Message::RepositoryLinkLabel { owner, name } => {
                self.literal(&mut output, "Open repository ");
                self.argument(&mut output, owner);
                output.push('/');
                self.argument(&mut output, name);
            }
            Message::ExploreTitle => return self.literal_text("Explore"),
            Message::ExploreSubtitle { site } => {
                self.literal(&mut output, "Public repositories on ");
                self.argument(&mut output, site);
                output.push('.');
            }
            Message::ExploreSearchPlaceholder => return self.literal_text("Search by name or description"),
            Message::SortBy => return self.literal_text("Sort by"),
            Message::SortUpdated => return self.literal_text("Updated"),
            Message::SortName => return self.literal_text("Name"),
            Message::SortNewest => return self.literal_text("Newest"),
            Message::ArchiveFilterLabel => return self.literal_text("Archived repositories"),
            Message::ArchiveAll => return self.literal_text("All"),
            Message::ArchiveActive => return self.literal_text("Active"),
            Message::Archived => return self.literal_text("Archived"),
            Message::NothingHereYet => return self.literal_text("Nothing here yet"),
            Message::PublicRepositoriesEmpty => return self.literal_text("Public repositories will be listed here once they exist."),
            Message::NoMatchingRepositories => return self.literal_text("No matching repositories"),
            Message::SearchSuggestion => return self.literal_text("Try a shorter search or different words."),
            Message::CountForQuery { count, query } => {
                let _ = write!(output, "{count}");
                self.literal(&mut output, if count == 1 { " repository" } else { " repositories" });
                if !query.is_empty() {
                    self.literal(&mut output, " for “");
                    self.argument(&mut output, query);
                    output.push('”');
                }
            }
            Message::PaginationLabel => return self.literal_text("Pagination"),
            Message::PageOf { page, pages } => {
                self.literal(&mut output, "Page ");
                let _ = write!(output, "{page}");
                self.literal(&mut output, " of ");
                let _ = write!(output, "{pages}");
            }
            Message::Stars { count } => {
                let _ = write!(output, "{count}");
                self.literal(&mut output, " stars");
            }
        }
        self.finish(output)
    }

    fn literal_text(self, text: &'static str) -> LocalizedText {
        if self.locale == Locale::English {
            return LocalizedText(Cow::Borrowed(text));
        }
        let mut output = String::new();
        self.literal(&mut output, text);
        self.finish(output)
    }

    fn finish(self, mut output: String) -> LocalizedText {
        if self.locale != Locale::English {
            output.insert(0, '[');
            output.push(']');
        }
        LocalizedText(Cow::Owned(output))
    }

    fn literal(self, output: &mut String, text: &str) {
        match self.locale {
            Locale::PseudoExpanded => pseudo::expand(output, text),
            Locale::English | Locale::PseudoRtl => output.push_str(text),
        }
    }

    fn argument(self, output: &mut String, value: &str) {
        if self.locale == Locale::PseudoRtl {
            output.push('\u{2068}');
        }
        output.push_str(value);
        if self.locale == Locale::PseudoRtl {
            output.push('\u{2069}');
        }
    }
}

#[cfg(test)]
mod tests;
