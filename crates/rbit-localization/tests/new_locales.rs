//! The locales prepared for translators: a message without an entry falls back to its
//! English text, and every family message is marked as such.
use rbit_localization::{
    ActionsJobMessage, ActionsMessage, Catalog, ChromeMessage, IssueMessage, LanguageMessage, Locale, Localized, Message,
    SettingsNavMessage, ThemeMessage,
};

const NEW: [Locale; 7] = [
    Locale::ChineseSimplified,
    Locale::PortugueseBrazil,
    Locale::Japanese,
    Locale::German,
    Locale::Russian,
    Locale::French,
    Locale::Korean,
];

/// The text must follow its mark: English marked `en`, or a translation in the locale.
fn assert_marked(text: &Localized, source: &Localized, locale: Locale) {
    if text.fallback() {
        assert_eq!(text.as_str(), source.as_str(), "{locale:?}");
        assert_eq!(text.marked_lang(), Some("en"), "{locale:?}");
    } else {
        assert_eq!(text.marked_lang(), Some(locale.tag()), "{locale:?}");
    }
}

#[test]
fn the_formerly_exhaustive_families_are_table_backed_in_every_new_locale() {
    let english = Catalog::new(Locale::English);
    for locale in NEW {
        let catalog = Catalog::new(locale);
        for message in [ChromeMessage::Skip, ChromeMessage::SignIn, ChromeMessage::Home { site: "Forge" }] {
            assert_marked(&catalog.localized(message), &english.localized(message), locale);
            assert_eq!(catalog.chrome_display(message).to_string(), catalog.localized(message).as_str());
        }
        for message in [Message::ExploreTitle, Message::RepositoryCount { count: 2 }, Message::Stars { count: -1 }] {
            assert_marked(&catalog.localized(message), &english.localized(message), locale);
            assert_eq!(catalog.text(message).as_str(), catalog.localized(message).as_str());
            assert_eq!(catalog.display(message).to_string(), catalog.localized(message).as_str());
        }
        assert_marked(&catalog.localized(LanguageMessage::Title), &english.localized(LanguageMessage::Title), locale);
        assert_marked(&catalog.localized(ThemeMessage::Choose), &english.localized(ThemeMessage::Choose), locale);
        assert_marked(&catalog.localized(ActionsMessage::RunDetails), &english.localized(ActionsMessage::RunDetails), locale);
        let steps = ActionsJobMessage::StepCount { count: 3 };
        assert_marked(&catalog.localized(steps), &english.localized(steps), locale);
    }
}

#[test]
fn family_messages_fall_back_marked_english_in_every_new_locale() {
    let english = Catalog::new(Locale::English);
    for locale in NEW {
        let catalog = Catalog::new(locale);
        let messages = [
            (catalog.localized(IssueMessage::NewIssue), english.localized(IssueMessage::NewIssue)),
            (catalog.localized(SettingsNavMessage::Profile), english.localized(SettingsNavMessage::Profile)),
        ];
        // A locale may have translated these; whichever it is, the mark must follow.
        for (text, source) in messages {
            if text.fallback() {
                assert_eq!(text.as_str(), source.as_str(), "{locale:?}");
                assert_eq!(text.marked_lang(), Some("en"), "{locale:?}");
            } else {
                assert_eq!(text.marked_lang(), Some(locale.tag()), "{locale:?}");
            }
        }
        let timestamp = Localized::english_only("2 hours ago".to_owned(), locale);
        assert_eq!(timestamp.marked_lang(), Some("en"), "{locale:?}");
    }
}

#[test]
fn text_in_the_page_language_of_an_original_catalog_needs_no_mark() {
    let spanish = Catalog::new(Locale::Spanish).localized(SettingsNavMessage::Profile);
    assert!(!spanish.fallback());
    assert_eq!(spanish.marked_lang(), None);
    let english = Catalog::new(Locale::English).localized(IssueMessage::NewIssue);
    assert_eq!(english.marked_lang(), None);
    assert_eq!(Catalog::new(Locale::PseudoRtl).localized(IssueMessage::NewIssue).marked_lang(), None);
}
