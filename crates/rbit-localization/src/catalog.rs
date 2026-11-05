//! Compiled messages for the first repository-listing slice; no runtime parsing.

use std::{fmt, fmt::Write};

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
        }
    }
}

/// Plain Unicode text, never an HTML fragment. Use ordinary Askama interpolation;
/// `safe` or unescaped `innerHTML` would violate this type's presentation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText(String);

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
        if self.locale != Locale::English {
            output.push('[');
        }
        match message {
            Message::RepositoriesTitle => self.literal(&mut output, "Repositories"),
            Message::RepositoryCount { count } => {
                let _ = write!(output, "{count}");
                self.literal(&mut output, if count == 1 { " repository" } else { " repositories" });
            }
            Message::OwnerRepositories { owner } => {
                self.literal(&mut output, "Repositories owned by ");
                self.argument(&mut output, owner);
            }
            Message::Visibility { visibility } => self.literal(
                &mut output,
                match visibility {
                    RepositoryVisibility::Public => "Public",
                    RepositoryVisibility::Private => "Private",
                    RepositoryVisibility::Internal => "Internal",
                },
            ),
            Message::EmptyRepositories => self.literal(&mut output, "No repositories found."),
            Message::SearchRepositories => self.literal(&mut output, "Search repositories"),
            Message::SearchLabel => self.literal(&mut output, "Search"),
            Message::NextPage => self.literal(&mut output, "Next"),
            Message::PreviousPage => self.literal(&mut output, "Previous"),
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
        }
        if self.locale != Locale::English {
            output.push(']');
        }
        LocalizedText(output)
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
