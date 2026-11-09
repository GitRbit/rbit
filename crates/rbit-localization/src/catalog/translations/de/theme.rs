//! German (`de`) entries for `catalog/theme.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;
use crate::catalog::theme::ThemeSentence;
use crate::plural::Plural;

entries! { De:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("Design auswählen"),
        ThemeMessage::Search => text.literal("Designs durchsuchen"),
        ThemeMessage::Close => text.literal("Schließen"),
        ThemeMessage::Show => text.literal("Anzeigen"),
        ThemeMessage::All => text.literal("Alle"),
        ThemeMessage::Dark => text.literal("Dunkel"),
        ThemeMessage::Light => text.literal("Hell"),
        ThemeMessage::Colorblind => text.literal("Farbenblindheit"),
        ThemeMessage::Families => text.literal("Design-Familien"),
        ThemeMessage::Recent => text.literal("Zuletzt verwendet"),
        ThemeMessage::Themes => text.literal("Designs"),
        ThemeMessage::Empty => text.literal("Keine Designs passen zu Ihrer Suche."),
        ThemeMessage::Preview => text.literal("Vorschau"),
        ThemeMessage::System => text.literal("System"),
        ThemeMessage::BuiltIn => text.literal("Integriert"),
        ThemeMessage::More => text.literal("Weitere Designs"),
        ThemeMessage::Auto => text.literal("Automatisch"),
        ThemeMessage::Apply => text.literal("Übernehmen"),
        ThemeMessage::Applied => text.literal("Übernommen"),
        ThemeMessage::By => text.literal("Von"),
        ThemeMessage::Credit => text.literal("In rbit integriert"),
        ThemeMessage::OrApply => text.literal("oder auf Übernehmen klicken"),
    }
    ThemeSentence => RichWriter |text| {
        ThemeSentence::Count { count } => {
            text.slot(0);
            text.literal(match text.plural(count) {
                Plural::One => " Design",
                _ => " Designs",
            });
        }
    }
}
