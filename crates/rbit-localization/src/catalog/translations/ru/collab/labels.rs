//! Russian (`ru`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("Новая метка"),
        LabelMessage::Name => text.literal("Название"),
        LabelMessage::Colour => text.literal("Цвет"),
        LabelMessage::CreateLabel => text.literal("Создать метку"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "метка",
                Plural::Few => "метки",
                _ => "меток",
            });
        }
        LabelMessage::EmptyTitle => text.literal("Меток пока нет"),
        LabelMessage::EmptyText => text.literal("Метки упорядочивают задачи и запросы на слияние: ошибка, улучшение, хорошая первая задача…"),
        LabelMessage::CreateDefaults => text.literal("Создать стандартные метки"),
        LabelMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "открытая задача",
                Plural::Few => "открытые задачи",
                _ => "открытых задач",
            });
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("Удалить метку «");
            text.argument(name);
            text.literal("»? Она будет снята со всех задач.");
        }
    }
}
