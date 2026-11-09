//! Russian (`ru`) entries for `catalog/theme.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;

entries! { Ru:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("Выбрать тему"),
        ThemeMessage::Search => text.literal("Поиск тем"),
        ThemeMessage::Close => text.literal("Закрыть"),
        ThemeMessage::Show => text.literal("Показать"),
        ThemeMessage::All => text.literal("Все"),
        ThemeMessage::Dark => text.literal("Тёмные"),
        ThemeMessage::Light => text.literal("Светлые"),
        ThemeMessage::Colorblind => text.literal("Для дальтоников"),
        ThemeMessage::Families => text.literal("Семейства тем"),
        ThemeMessage::Recent => text.literal("Недавние"),
        ThemeMessage::Themes => text.literal("Темы"),
        ThemeMessage::Empty => text.literal("Нет тем, соответствующих запросу."),
        ThemeMessage::Preview => text.literal("Предпросмотр"),
        ThemeMessage::System => text.literal("Системная"),
        ThemeMessage::BuiltIn => text.literal("Встроенные"),
        ThemeMessage::More => text.literal("Другие темы"),
        ThemeMessage::Auto => text.literal("Автоматически"),
        ThemeMessage::Apply => text.literal("Применить"),
        ThemeMessage::Applied => text.literal("Применена"),
        ThemeMessage::By => text.literal("Автор:"),
        ThemeMessage::Credit => text.literal("Встроена в rbit"),
        ThemeMessage::ThemeSingular => text.literal("тема"),
        ThemeMessage::ThemePlural => text.literal("тем"),
        ThemeMessage::SearchPrefix => text.literal("Поиск среди"),
        ThemeMessage::OrApply => text.literal("или нажмите «Применить»"),
    }
}
