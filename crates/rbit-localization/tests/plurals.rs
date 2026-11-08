//! CLDR plural categories for whole numbers in every catalog, and the locale identities.
use rbit_localization::{Direction, Locale, Plural};

fn categories(locale: Locale, counts: &[u64]) -> Vec<Plural> {
    counts.iter().map(|&count| Plural::of(locale, count)).collect()
}

#[test]
fn english_and_german_have_one_for_exactly_one() {
    for locale in [Locale::English, Locale::German, Locale::PseudoExpanded, Locale::PseudoRtl] {
        assert_eq!(
            categories(locale, &[0, 1, 2, 11, 21, 101, 1_000_000]),
            [Plural::Other, Plural::One, Plural::Other, Plural::Other, Plural::Other, Plural::Other, Plural::Other],
            "{locale:?}"
        );
    }
}

#[test]
fn spanish_has_one_for_one_and_many_for_whole_millions() {
    assert_eq!(
        categories(Locale::Spanish, &[0, 1, 2, 21, 999_999, 1_000_000, 2_000_000, 1_000_001]),
        [Plural::Other, Plural::One, Plural::Other, Plural::Other, Plural::Other, Plural::Many, Plural::Many, Plural::Other]
    );
}

#[test]
fn french_and_brazilian_portuguese_treat_zero_and_one_as_one() {
    for locale in [Locale::French, Locale::PortugueseBrazil] {
        assert_eq!(
            categories(locale, &[0, 1, 2, 17, 1_000_000, 3_000_000, 1_000_002]),
            [Plural::One, Plural::One, Plural::Other, Plural::Other, Plural::Many, Plural::Many, Plural::Other],
            "{locale:?}"
        );
    }
}

#[test]
fn russian_distinguishes_one_few_and_many() {
    let one = [1, 21, 31, 101, 1_001];
    let few = [2, 3, 4, 22, 24, 102, 1_004];
    let many = [0, 5, 9, 10, 11, 12, 13, 14, 15, 20, 25, 100, 111, 112, 114, 1_000_000];
    assert!(one.iter().all(|&count| Plural::of(Locale::Russian, count) == Plural::One));
    assert!(few.iter().all(|&count| Plural::of(Locale::Russian, count) == Plural::Few));
    assert!(many.iter().all(|&count| Plural::of(Locale::Russian, count) == Plural::Many));
}

#[test]
fn chinese_japanese_and_korean_have_only_other() {
    for locale in [Locale::ChineseSimplified, Locale::Japanese, Locale::Korean] {
        assert!((0..=1_000).chain([1_000_000]).all(|count| Plural::of(locale, count) == Plural::Other), "{locale:?}");
        assert_eq!(Plural::categories(locale), [Plural::Other]);
    }
}

#[test]
fn every_category_a_locale_produces_is_one_it_declares() {
    for locale in Locale::LANGUAGES.into_iter().chain([Locale::PseudoExpanded, Locale::PseudoRtl]) {
        let declared = Plural::categories(locale);
        for count in (0..=2_000).chain([1_000_000, 2_000_000, 10_000_021]) {
            assert!(declared.contains(&Plural::of(locale, count)), "{locale:?} {count}");
        }
        for category in declared {
            assert!((0..=2_000).chain([1_000_000]).any(|count| Plural::of(locale, count) == *category), "{locale:?} {category:?} occurs");
        }
    }
}

#[test]
fn languages_have_canonical_tags_native_names_and_left_to_right_text() {
    let expected = [
        (Locale::English, "en", "English"),
        (Locale::German, "de", "Deutsch"),
        (Locale::Spanish, "es", "Español"),
        (Locale::French, "fr", "Français"),
        (Locale::PortugueseBrazil, "pt-BR", "Português (Brasil)"),
        (Locale::Russian, "ru", "Русский"),
        (Locale::Japanese, "ja", "日本語"),
        (Locale::ChineseSimplified, "zh-Hans", "简体中文"),
        (Locale::Korean, "ko", "한국어"),
    ];
    assert_eq!(Locale::LANGUAGES.len(), expected.len());
    for (locale, (expected_locale, tag, name)) in Locale::LANGUAGES.into_iter().zip(expected) {
        assert_eq!(locale, expected_locale);
        assert_eq!(locale.tag(), tag);
        assert_eq!(locale.native_name(), name);
        assert_eq!(locale.catalog_key(), format!("1.{tag}"));
        assert_eq!(locale.direction(), Direction::LeftToRight);
    }
    assert_eq!(Locale::PseudoRtl.direction(), Direction::RightToLeft);
}
