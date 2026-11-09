//! Real compiled picker catalogs retain static English/Spanish labels.
use rbit_localization::{Catalog, Locale, ThemeMessage, ThemeSentence};

#[test]
fn theme_labels_are_compiled_and_machine_values_are_not_catalog_messages() {
    for (message, english, spanish) in [
        (ThemeMessage::Choose, "Choose a theme", "Elegir un tema"),
        (ThemeMessage::Colorblind, "Colorblind", "Daltonismo"),
        (ThemeMessage::Search, "Search themes", "Buscar temas"),
        (ThemeMessage::Applied, "Applied", "Aplicado"),
    ] {
        assert_eq!(Catalog::new(Locale::English).localized(message).as_str(), english);
        assert_eq!(Catalog::new(Locale::Spanish).localized(message).as_str(), spanish);
    }
}

#[test]
fn the_theme_count_is_one_sentence_with_each_language_plural_forms() {
    for (count, english, spanish, russian) in [
        (1, "1 theme", "1 tema", "1 тема"),
        (3, "3 themes", "3 temas", "3 темы"),
        (5, "5 themes", "5 temas", "5 тем"),
        (21, "21 themes", "21 temas", "21 тема"),
    ] {
        let digits = count.to_string();
        let text = |locale| ThemeSentence::Count { count }.rich(locale).plain(&[digits.as_str()]);
        assert_eq!((text(Locale::English), text(Locale::Spanish), text(Locale::Russian)), (english.into(), spanish.into(), russian.into()));
    }
    assert_eq!(ThemeSentence::Count { count: 4 }.rich(Locale::Korean).plain(&["4"]), "테마 4개");
}
