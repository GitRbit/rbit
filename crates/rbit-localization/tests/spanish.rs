//! Fixed translation expectations complement the owned/display parity oracle.
use rbit_localization::{
    ActionsJobMessage, ActionsMessage, Catalog, ChromeMessage, LanguageMessage, Locale, LocalePreferences, LocaleResolver, Message,
    PreferenceSource,
};

#[test]
fn real_spanish_is_available_without_enabling_diagnostic_catalogs() {
    let resolver = LocaleResolver::default();
    for language in ["es", "ES-mx", "es-Latn-ES", "es-ES-u-nu-latn"] {
        let selected = resolver.resolve(LocalePreferences { accept_language: Some(language), ..Default::default() });
        assert_eq!(selected.locale, Locale::Spanish);
        assert_eq!(selected.source, PreferenceSource::AcceptLanguage);
    }
    assert_eq!(
        resolver.resolve(LocalePreferences { accept_language: Some("es;q=0, en;q=0.5"), ..Default::default() }).locale,
        Locale::English
    );
    assert_eq!(
        resolver.resolve(LocalePreferences { account: Some("en"), anonymous: Some("es"), ..Default::default() }).locale,
        Locale::English
    );
    assert_eq!(
        resolver.resolve(LocalePreferences { explicit: Some("ar-XB"), account: Some("es"), ..Default::default() }).locale,
        Locale::Spanish
    );
}

#[test]
fn typed_catalog_has_real_literals_and_explicit_integer_noun_forms() {
    let catalog = Catalog::new(Locale::Spanish);
    assert_eq!(catalog.text(Message::RepositoriesTitle).as_str(), "Repositorios");
    assert_eq!(catalog.localized(ChromeMessage::SignIn).as_str(), "Iniciar sesión");
    assert_eq!(catalog.localized(LanguageMessage::Title).as_str(), "Idioma de la interfaz");
    for (count, expected) in [(0, "0 repositorios"), (1, "1 repositorio"), (2, "2 repositorios"), (1_000_000, "1.000.000 repositorios")] {
        assert_eq!(catalog.text(Message::RepositoryCount { count }).as_str(), expected);
    }
    for (count, expected) in [(0, "0 estrellas"), (-1, "-1 estrella"), (1, "1 estrella"), (2, "2 estrellas")] {
        assert_eq!(catalog.display(Message::Stars { count }).to_string(), expected);
    }
    assert_eq!(catalog.actions_display(ActionsMessage::RerunJob { name: "<build>" }).to_string(), "Repetir <build>");
    assert_eq!(catalog.job_display(ActionsJobMessage::StepCount { count: 1 }).to_string(), "1 paso");
    assert_eq!(catalog.job_display(ActionsJobMessage::StepCount { count: 2 }).to_string(), "2 pasos");
}

#[test]
fn chrome_dynamic_labels_match_plain_text_and_preserve_untrusted_arguments() {
    for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let catalog = Catalog::new(locale);
        for site in ["", "Forge", "<script>&\"'雪\u{2069}"] {
            let message = ChromeMessage::Home { site };
            assert_eq!(catalog.chrome_display(message).to_string(), catalog.localized(message).as_str());
        }
    }
}
