//! The error and offline pages, the shared repository row and pagination, and public
//! account profiles.
use super::phrase;
use crate::catalog::fallback::{MessageFamily, MessageWriter};

/// Why a request failed, as the error page names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    NotFound,
    Forbidden,
    BadRequest,
    SlowDown,
    Unavailable,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageMessage<'a> {
    ErrorTitle(Failure),
    NotFoundText,
    UnavailableText,
    InternalText,
    GoHome,
    Explore,
    Offline,
    OfflineText,
    Retry,
    Private,
    Archived,
    /// A repository's star count.
    Stars(i64),
    Pagination,
    Previous,
    Next,
    /// The current page and the number of pages.
    PageOf(u32, u32),
    EditProfile,
    /// When the account was created, already formatted.
    Joined(&'a str),
    Profile,
    Repositories,
    StarsTab,
    NewRepository,
    /// The account's name.
    NoStars(&'a str),
    /// The account's name.
    NoPublicRepositories(&'a str),
}

impl MessageFamily for PageMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::ErrorTitle(failure) => match failure {
                Failure::NotFound => "error.not-found",
                Failure::Forbidden => "error.forbidden",
                Failure::BadRequest => "error.bad-request",
                Failure::SlowDown => "error.slow-down",
                Failure::Unavailable => "error.unavailable",
                Failure::Internal => "error.internal",
            },
            Self::NotFoundText => "error.not-found-text",
            Self::UnavailableText => "error.unavailable-text",
            Self::InternalText => "error.internal-text",
            Self::GoHome => "error.go-home",
            Self::Explore => "error.explore",
            Self::Offline => "offline.title",
            Self::OfflineText => "offline.text",
            Self::Retry => "offline.retry",
            Self::Private => "repository-row.private",
            Self::Archived => "repository-row.archived",
            Self::Stars(_) => "repository-row.stars",
            Self::Pagination => "pager.label",
            Self::Previous => "pager.previous",
            Self::Next => "pager.next",
            Self::PageOf(..) => "pager.page-of",
            Self::EditProfile => "profile.edit",
            Self::Joined(_) => "profile.joined",
            Self::Profile => "profile.tabs",
            Self::Repositories => "profile.repositories",
            Self::StarsTab => "profile.stars",
            Self::NewRepository => "profile.new-repository",
            Self::NoStars(_) => "profile.no-stars",
            Self::NoPublicRepositories(_) => "profile.no-repositories",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::Stars(count) => {
                text.number(count.unsigned_abs());
                if count == 1 { " star" } else { " stars" }
            }
            Self::PageOf(page, pages) => {
                text.literal("Page ");
                text.number(u64::from(page));
                text.literal(" of ");
                return text.number(u64::from(pages));
            }
            Self::Joined(date) => return phrase(text, "Joined ", date, ""),
            Self::NoStars(name) => return phrase(text, "", name, " hasn't starred any repositories yet."),
            Self::NoPublicRepositories(name) => return phrase(text, "", name, " has no public repositories yet."),
            Self::ErrorTitle(failure) => match failure {
                Failure::NotFound => "Page not found",
                Failure::Forbidden => "Access denied",
                Failure::BadRequest => "Bad request",
                Failure::SlowDown => "Slow down",
                Failure::Unavailable => "Temporarily unavailable",
                Failure::Internal => "Something went wrong",
            },
            Self::NotFoundText => "The page you were looking for doesn't exist, or you don't have access to it.",
            Self::UnavailableText => "The service is temporarily unavailable. Try again shortly.",
            Self::InternalText => "An unexpected error occurred. It has been logged.",
            Self::GoHome => "Go home",
            Self::Explore => "Explore",
            Self::Offline => "You're offline",
            Self::OfflineText => "The server couldn't be reached. Check your connection and try again.",
            Self::Retry => "Retry",
            Self::Private => "Private",
            Self::Archived => "Archived",
            Self::Pagination => "Pagination",
            Self::Previous => "Previous",
            Self::Next => "Next",
            Self::EditProfile => "Edit profile",
            Self::Profile => "Profile",
            Self::Repositories => "Repositories",
            Self::StarsTab => "Stars",
            Self::NewRepository => "New repository",
        };
        text.literal(literal);
    }
}
