//! The fallback mechanism: a message family needs only English, untranslated messages
//! render the English text and say so, and English-derived catalogs never fall back.
use rbit_localization::{Catalog, Locale, Localized, MessageFamily, MessageWriter};

/// A new family, or a new key, compiles with nothing but its English text: no other
/// catalog has to add an entry, and those locales fall back until one does.
#[derive(Clone, Copy, Debug)]
enum NewFeature {
    Greeting,
    Welcome { name: &'static str },
}

impl MessageFamily for NewFeature {
    fn id(self) -> &'static str {
        match self {
            Self::Greeting => "new.greeting",
            Self::Welcome { .. } => "new.welcome",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::Greeting => text.literal("Hello"),
            Self::Welcome { name } => {
                text.literal("Welcome, ");
                text.argument(name);
            }
        }
    }
}

#[test]
fn adding_a_key_never_requires_a_non_english_entry() {
    for message in [NewFeature::Greeting, NewFeature::Welcome { name: "Ana" }] {
        let english = Catalog::new(Locale::English).localized(message);
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert_eq!(spanish.as_str(), english.as_str(), "{message:?}");
        assert!(spanish.fallback(), "{message:?} falls back in Spanish");
    }
    let welcome = Catalog::new(Locale::Spanish).localized(NewFeature::Welcome { name: "<Ana>" });
    assert_eq!(welcome.as_str(), "Welcome, <Ana>", "arguments stay plain text");
}

#[test]
fn english_text_is_never_marked_as_a_fallback() {
    let english = Catalog::new(Locale::English).localized(NewFeature::Greeting);
    assert_eq!(english.as_str(), "Hello");
    assert!(!english.fallback());
}

#[test]
fn diagnostic_catalogs_still_derive_from_english() {
    let expanded = Catalog::new(Locale::PseudoExpanded).localized(NewFeature::Greeting);
    assert_ne!(expanded.as_str(), "Hello", "expansion rewrites the English literal");
    assert!(expanded.as_str().starts_with('[') && expanded.as_str().ends_with(']'));
    assert!(!expanded.fallback());
    let rtl = Catalog::new(Locale::PseudoRtl).localized(NewFeature::Welcome { name: "Ana" });
    assert!(rtl.as_str().contains("\u{2068}Ana\u{2069}"), "arguments are isolated: {rtl}");
    assert!(!rtl.fallback());
}

#[test]
fn english_only_text_is_a_fallback_only_in_translated_locales() {
    for (locale, fallback) in
        [(Locale::English, false), (Locale::Spanish, true), (Locale::PseudoExpanded, false), (Locale::PseudoRtl, false)]
    {
        let text = Localized::english_only("2 hours ago".to_owned(), locale);
        assert_eq!(text.as_str(), "2 hours ago");
        assert_eq!(text.fallback(), fallback, "{locale:?}");
    }
}
