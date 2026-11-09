use rbit_localization::{Locale, LocalePreferences, LocaleResolver, PreferenceSource};

fn negotiate(header: &str) -> rbit_localization::ResolvedLocale {
    LocaleResolver::new(Locale::English, true)
        .unwrap()
        .resolve(LocalePreferences { accept_language: Some(header), ..LocalePreferences::default() })
}

#[test]
fn explicit_account_anonymous_and_header_have_strict_priority() {
    let resolver = LocaleResolver::new(Locale::English, true).unwrap();
    let mut choices =
        LocalePreferences { explicit: Some("ar-XB"), account: Some("en-XA"), anonymous: Some("en"), accept_language: Some("ar-XB") };
    assert_eq!(resolver.resolve(choices).source, PreferenceSource::Explicit);
    assert_eq!(resolver.resolve(choices).locale, Locale::PseudoRtl);
    choices.explicit = Some("unsupported");
    assert_eq!(resolver.resolve(choices).source, PreferenceSource::Account);
    choices.account = None;
    assert_eq!(resolver.resolve(choices).source, PreferenceSource::Anonymous);
    choices.anonymous = Some("en<script>");
    assert_eq!(resolver.resolve(choices).source, PreferenceSource::AcceptLanguage);
}

#[test]
fn weighted_ranges_ties_exclusions_and_specificity_are_deterministic() {
    assert_eq!(negotiate("ar-XB;q=0.8, en-XA;q=0.9").locale, Locale::PseudoExpanded);
    assert_eq!(negotiate("ar-XB;q=0.8, en-XA;q=0.8").locale, Locale::PseudoRtl);
    assert_eq!(negotiate("en-XA;q=0, en;q=0.3").locale, Locale::English);
    assert_eq!(negotiate("en;q=0, en-US;q=0.9").source, PreferenceSource::AcceptLanguage);
    assert_eq!(negotiate("*;q=1, en;q=0, ar-XB;q=0.4").locale, Locale::PseudoRtl);
    assert_eq!(negotiate("en;q=0").source, PreferenceSource::Default);
    assert_eq!(negotiate("en;q=0, en;q=1").source, PreferenceSource::Default);
    assert_eq!(negotiate("*").locale, Locale::English);
}

#[test]
fn language_lookup_is_case_insensitive_and_handles_extensions() {
    for header in ["EN-us", "en-Latn-US", "en-US-u-ca-gregory", "en-XA-u-nu-latn"] {
        assert_eq!(negotiate(header).source, PreferenceSource::AcceptLanguage, "{header}");
    }
    assert_eq!(negotiate("en-XA-u-nu-latn").locale, Locale::PseudoExpanded);
    assert_eq!(negotiate("it-IT, nl").source, PreferenceSource::Default);
}

#[test]
fn malformed_weights_cannot_become_high_priority_choices() {
    for weight in ["NaN", "1.001", "2", "-1", ".9", "0.1234", "01", "1e0"] {
        let header = format!("ar-XB;q={weight},en;q=0.1");
        assert_eq!(negotiate(&header).locale, Locale::English, "{header}");
    }
    for header in ["ar-XB;q=NaN", "ar-XB;q=1.001", "ar-XB;bad=1", "ar-XB;q=1;q=1"] {
        assert_eq!(negotiate(header).source, PreferenceSource::Default, "{header}");
    }
    assert_eq!(negotiate("ar-XB;Q=1.000").locale, Locale::PseudoRtl);
    assert_eq!(negotiate("ar-XB;q=0.001").locale, Locale::PseudoRtl);
    assert_eq!(negotiate("ar-XB;q=0.").source, PreferenceSource::Default);
}

#[test]
fn oversized_or_injected_headers_fall_back_without_partial_selection() {
    for header in [
        format!("ar-XB,{}", "a".repeat(4096)),
        format!("ar-XB,{}", "en,".repeat(32)),
        "ar-XB\r\nInjected: yes".into(),
        "ar-XB, 日本語".into(),
        "ar-XB\0".into(),
    ] {
        assert_eq!(negotiate(&header).source, PreferenceSource::Default);
    }
    for header in ["en_Us", "en--US", "en-123456789", "en/<script>", "*-XB", ""] {
        assert_eq!(negotiate(header).source, PreferenceSource::Default, "{header}");
    }
}

#[test]
fn pseudo_locales_are_opt_in_and_default_configuration_is_validated() {
    let resolver = LocaleResolver::default();
    let selected = resolver.resolve(LocalePreferences { explicit: Some("en-XA"), ..LocalePreferences::default() });
    assert_eq!(selected.source, PreferenceSource::Default);
    assert_eq!(
        resolver.resolve(LocalePreferences { accept_language: Some("ar-XB"), ..LocalePreferences::default() }).source,
        PreferenceSource::Default
    );
    assert!(LocaleResolver::new(Locale::PseudoRtl, false).is_err());
    let rtl = LocaleResolver::new(Locale::PseudoRtl, true).unwrap();
    assert_eq!(rtl.resolve(LocalePreferences::default()).locale, Locale::PseudoRtl);
}

fn production(header: &str) -> Locale {
    LocaleResolver::default().resolve(LocalePreferences { accept_language: Some(header), ..LocalePreferences::default() }).locale
}

#[test]
fn every_language_negotiates_from_its_browser_ranges() {
    for (header, expected) in [
        ("de-DE,de;q=0.9,en;q=0.5", Locale::German),
        ("de-AT", Locale::German),
        ("fr-CA", Locale::French),
        ("fr-FR, en;q=0.1", Locale::French),
        ("ru-RU", Locale::Russian),
        ("ja-JP", Locale::Japanese),
        ("ko-KR", Locale::Korean),
        ("es-419", Locale::Spanish),
        ("pt-BR", Locale::PortugueseBrazil),
        ("pt-PT", Locale::PortugueseBrazil),
        ("pt", Locale::PortugueseBrazil),
        ("zh-CN", Locale::ChineseSimplified),
        ("zh-SG", Locale::ChineseSimplified),
        ("zh-MY", Locale::ChineseSimplified),
        ("zh", Locale::ChineseSimplified),
        ("zh-Hans", Locale::ChineseSimplified),
        ("ZH-hans-TW", Locale::ChineseSimplified),
        ("zh-cmn-Hans-CN", Locale::ChineseSimplified),
        ("ja;q=0.4, zh-CN;q=0.8", Locale::ChineseSimplified),
    ] {
        assert_eq!(production(header), expected, "{header}");
    }
}

#[test]
fn traditional_chinese_never_falls_back_to_simplified() {
    for header in ["zh-TW", "zh-HK", "zh-MO", "zh-Hant", "zh-Hant-CN", "zh-Hant-TW", "zh-yue-HK", "zh-TW, en;q=0.1"] {
        assert_eq!(production(header), Locale::English, "{header}");
    }
    assert_eq!(production("zh-TW, ja;q=0.5"), Locale::Japanese, "the next acceptable language wins");
}

#[test]
fn saved_choices_accept_only_known_language_tags() {
    let resolver = LocaleResolver::default();
    for locale in Locale::LANGUAGES {
        let chosen = resolver.resolve(LocalePreferences { account: Some(locale.tag()), ..LocalePreferences::default() });
        assert_eq!((chosen.locale, chosen.source), (locale, PreferenceSource::Account), "{locale:?}");
    }
    let traditional = resolver.resolve(LocalePreferences { account: Some("zh-Hant"), ..LocalePreferences::default() });
    assert_eq!(traditional.source, PreferenceSource::Default);
}
