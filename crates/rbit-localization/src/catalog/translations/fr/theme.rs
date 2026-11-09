//! French (`fr`) entries for `catalog/theme.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;
use crate::catalog::theme::ThemeSentence;
use crate::plural::Plural;

entries! { Fr:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("Choisir un thème"),
        ThemeMessage::Search => text.literal("Rechercher des thèmes"),
        ThemeMessage::Close => text.literal("Fermer"),
        ThemeMessage::Show => text.literal("Afficher"),
        ThemeMessage::All => text.literal("Tous"),
        ThemeMessage::Dark => text.literal("Sombre"),
        ThemeMessage::Light => text.literal("Clair"),
        ThemeMessage::Colorblind => text.literal("Daltonisme"),
        ThemeMessage::Families => text.literal("Familles de thèmes"),
        ThemeMessage::Recent => text.literal("Récents"),
        ThemeMessage::Themes => text.literal("Thèmes"),
        ThemeMessage::Empty => text.literal("Aucun thème ne correspond à votre recherche."),
        ThemeMessage::Preview => text.literal("Aperçu"),
        ThemeMessage::System => text.literal("Système"),
        ThemeMessage::BuiltIn => text.literal("Intégrés"),
        ThemeMessage::More => text.literal("Plus de thèmes"),
        ThemeMessage::Auto => text.literal("Automatique"),
        ThemeMessage::Apply => text.literal("Appliquer"),
        ThemeMessage::Applied => text.literal("Appliqué"),
        ThemeMessage::By => text.literal("Par"),
        ThemeMessage::Credit => text.literal("Intégré à rbit"),
        ThemeMessage::OrApply => text.literal("ou cliquez sur Appliquer"),
    }
    ThemeSentence => RichWriter |text| {
        ThemeSentence::Count { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " thème",
                _ => " thèmes",
            });
        }
    }
}
