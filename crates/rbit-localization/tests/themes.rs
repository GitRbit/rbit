//! Real compiled picker catalogs retain static English/Spanish labels.
use rbit_localization::{Catalog, Locale, ThemeMessage};

#[test]
fn theme_labels_are_compiled_and_machine_values_are_not_catalog_messages() {
    for (message, english, spanish) in [
        (ThemeMessage::Choose, "Choose a theme", "Elegir un tema"),
        (ThemeMessage::Colorblind, "Colorblind", "Daltonismo"),
        (ThemeMessage::Search, "Search themes", "Buscar temas"),
        (ThemeMessage::Applied, "Applied", "Aplicado"),
        (ThemeMessage::ThemeSingular, "theme", "tema"),
        (ThemeMessage::ThemePlural, "themes", "temas"),
    ] {
        assert_eq!(Catalog::new(Locale::English).theme(message).as_str(), english);
        assert_eq!(Catalog::new(Locale::Spanish).theme(message).as_str(), spanish);
    }
}
