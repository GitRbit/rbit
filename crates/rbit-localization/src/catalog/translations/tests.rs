//! A test-only family with real table entries: translated messages are used and are not
//! fallbacks, missing ones fall back, and plural arms follow the locale's CLDR rules.
use crate::catalog::Catalog;
use crate::catalog::fallback::{MessageFamily, MessageWriter};
use crate::locale::Locale;
use crate::plural::Plural;

#[derive(Clone, Copy, Debug)]
enum Probe<'a> {
    Hello,
    Files { count: u64 },
    Welcome { name: &'a str },
}

impl MessageFamily for Probe<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Hello => "probe.hello",
            Self::Files { .. } => "probe.files",
            Self::Welcome { .. } => "probe.welcome",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::Hello => text.literal("Hello"),
            Self::Files { count } => {
                text.number(count);
                text.literal(if count == 1 { " file" } else { " files" });
            }
            Self::Welcome { name } => {
                text.literal("Welcome, ");
                text.argument(name);
            }
        }
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        super::write(self, locale, text)
    }
}

entries! { De:
    Probe<'a> => MessageWriter |text| {
        Probe::Hello => text.literal("Hallo"),
        Probe::Files { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => "\u{a0}Datei",
                _ => "\u{a0}Dateien",
            });
        }
        Probe::Welcome { name } => {
            text.literal("Willkommen, ");
            text.argument(name);
        }
    }
}

entries! { Ru:
    Probe<'a> => MessageWriter |text| {
        Probe::Files { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " файл",
                Plural::Few => " файла",
                _ => " файлов",
            });
        }
    }
}

entries! { Es: Probe<'a> => MessageWriter |text| {} }
entries! { ZhHans: Probe<'a> => MessageWriter |text| {} }
entries! { PtBr: Probe<'a> => MessageWriter |text| {} }
entries! { Ja: Probe<'a> => MessageWriter |text| {} }
entries! { Fr: Probe<'a> => MessageWriter |text| {} }
entries! { Ko: Probe<'a> => MessageWriter |text| {} }

#[test]
fn a_table_entry_is_used_and_is_not_a_fallback() {
    let hello = Catalog::new(Locale::German).localized(Probe::Hello);
    assert_eq!(hello.as_str(), "Hallo");
    assert!(!hello.fallback());
    assert_eq!(hello.marked_lang(), Some("de"), "translated text declares its own language");
    let welcome = Catalog::new(Locale::German).localized(Probe::Welcome { name: "<Ana>" });
    assert_eq!(welcome.as_str(), "Willkommen, <Ana>", "arguments stay plain text");
}

#[test]
fn a_locale_without_the_entry_falls_back_to_marked_english() {
    for locale in [Locale::Spanish, Locale::French, Locale::ChineseSimplified, Locale::Japanese, Locale::Korean, Locale::PortugueseBrazil] {
        let hello = Catalog::new(locale).localized(Probe::Hello);
        assert_eq!(hello.as_str(), "Hello", "{locale:?}");
        assert!(hello.fallback(), "{locale:?}");
        assert_eq!(hello.marked_lang(), Some("en"), "{locale:?}");
    }
    let russian = Catalog::new(Locale::Russian).localized(Probe::Hello);
    assert!(russian.fallback(), "Russian has only a plural entry");
}

#[test]
fn plural_arms_follow_the_locale() {
    let german = |count| Catalog::new(Locale::German).localized(Probe::Files { count }).to_string();
    assert_eq!([german(0), german(1), german(2)], ["0\u{a0}Dateien", "1\u{a0}Datei", "2\u{a0}Dateien"]);
    let russian = |count| Catalog::new(Locale::Russian).localized(Probe::Files { count }).to_string();
    assert_eq!(
        [russian(1), russian(3), russian(5), russian(11), russian(21), russian(22)],
        ["1 файл", "3 файла", "5 файлов", "11 файлов", "21 файл", "22 файла"]
    );
}

#[test]
fn english_and_diagnostics_never_consult_a_table() {
    for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let hello = Catalog::new(locale).localized(Probe::Hello);
        assert!(!hello.fallback(), "{locale:?}");
        assert!(!hello.as_str().contains("Hallo"), "{locale:?}");
    }
    assert_eq!(Plural::of(Locale::German, 1), Plural::One);
}
