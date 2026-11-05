//! Representation checks keep static English labels allocation-free without exposing
//! storage details in the presentation API or relying on process-global allocators.

use super::{Catalog, Cow, Locale, Message, RepositoryVisibility};

const STATIC_MESSAGES: [Message<'static>; 20] = [
    Message::RepositoriesTitle,
    Message::EmptyRepositories,
    Message::SearchRepositories,
    Message::SearchLabel,
    Message::NextPage,
    Message::PreviousPage,
    Message::ExploreTitle,
    Message::ExploreSearchPlaceholder,
    Message::SortBy,
    Message::SortUpdated,
    Message::SortName,
    Message::SortNewest,
    Message::ArchiveFilterLabel,
    Message::ArchiveAll,
    Message::ArchiveActive,
    Message::Archived,
    Message::NothingHereYet,
    Message::PublicRepositoriesEmpty,
    Message::NoMatchingRepositories,
    Message::SearchSuggestion,
];

#[test]
fn english_static_labels_and_visibility_choices_borrow_compiled_text() {
    let catalog = Catalog::new(Locale::English);
    for message in STATIC_MESSAGES.into_iter().chain([Message::PaginationLabel]) {
        assert!(matches!(catalog.text(message).0, Cow::Borrowed(_)), "{}", message.id());
    }
    for visibility in [RepositoryVisibility::Public, RepositoryVisibility::Private, RepositoryVisibility::Internal] {
        assert!(matches!(catalog.text(Message::Visibility { visibility }).0, Cow::Borrowed(_)));
    }
}

#[test]
fn dynamic_messages_own_formatted_plain_text() {
    let catalog = Catalog::new(Locale::English);
    for message in [
        Message::RepositoryCount { count: 1 },
        Message::RepositoryCount { count: 2 },
        Message::OwnerRepositories { owner: "alice" },
        Message::PageNumber { page: 3 },
        Message::RepositoryLinkLabel { owner: "alice", name: "example" },
        Message::ExploreSubtitle { site: "Example Forge" },
        Message::CountForQuery { count: 2, query: "project" },
        Message::PageOf { page: 1, pages: 2 },
        Message::Stars { count: 10 },
    ] {
        assert!(matches!(catalog.text(message).0, Cow::Owned(_)), "{}", message.id());
    }
}

#[test]
fn pseudo_labels_always_own_their_transformed_output() {
    for locale in [Locale::PseudoExpanded, Locale::PseudoRtl] {
        let catalog = Catalog::new(locale);
        for message in STATIC_MESSAGES.into_iter().chain([Message::PaginationLabel, Message::CountForQuery { count: 2, query: "project" }])
        {
            let text = catalog.text(message);
            assert!(matches!(&text.0, Cow::Owned(_)), "{}", message.id());
            assert!(text.as_str().starts_with('[') && text.as_str().ends_with(']'));
        }
    }
}
