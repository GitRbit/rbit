//! The locales prepared for translators: the original exhaustive families fall back to
//! their English text without new arms, and every family message is marked as such.
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

#[test]
fn original_families_render_their_english_text_in_the_new_locales() {
    let english = Catalog::new(Locale::English);
    for locale in NEW {
        let catalog = Catalog::new(locale);
        assert!(!locale.has_original_catalog(), "{locale:?}");
        for message in [ChromeMessage::Skip, ChromeMessage::SignIn, ChromeMessage::Home { site: "Forge" }] {
            assert_eq!(catalog.chrome(message), english.chrome(message), "{locale:?} {message:?}");
            assert_eq!(catalog.chrome_display(message).to_string(), english.chrome_display(message).to_string());
        }
        for message in [Message::ExploreTitle, Message::RepositoryCount { count: 2 }, Message::Stars { count: -1 }] {
            assert_eq!(catalog.text(message), english.text(message), "{locale:?} {message:?}");
            assert_eq!(catalog.display(message).to_string(), english.display(message).to_string());
        }
        assert_eq!(catalog.language(LanguageMessage::Title), english.language(LanguageMessage::Title));
        assert_eq!(catalog.theme(ThemeMessage::Choose), english.theme(ThemeMessage::Choose));
        assert_eq!(catalog.actions(ActionsMessage::RunDetails), english.actions(ActionsMessage::RunDetails));
        assert_eq!(catalog.job(ActionsJobMessage::StepCount { count: 3 }), english.job(ActionsJobMessage::StepCount { count: 3 }));
    }
    for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
        assert!(locale.has_original_catalog(), "{locale:?}");
    }
}

#[test]
fn family_messages_fall_back_marked_english_in_every_new_locale() {
    for locale in NEW {
        let text = Catalog::new(locale).localized(IssueMessage::NewIssue);
        assert_eq!(text.as_str(), Catalog::new(Locale::English).localized(IssueMessage::NewIssue).as_str());
        assert!(text.fallback(), "{locale:?}");
        assert_eq!(text.marked_lang(), Some("en"), "{locale:?}");
        let nav = Catalog::new(locale).localized(SettingsNavMessage::Profile);
        assert!(nav.fallback(), "{locale:?} has no settings navigation entries yet");
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
