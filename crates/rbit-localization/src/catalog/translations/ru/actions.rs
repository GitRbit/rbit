//! Russian (`ru`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("Рабочий процесс"),
        ActionsMessage::Commit => text.literal("Коммит"),
        ActionsMessage::Duration => text.literal("Длительность"),
        ActionsMessage::Attempt => text.literal("Попытка"),
        ActionsMessage::TriggeredBy => text.literal("Инициатор"),
        ActionsMessage::Jobs => text.literal("Задания"),
        ActionsMessage::Artifacts => text.literal("Артефакты"),
        ActionsMessage::RunDetails => text.literal("Сведения о запуске"),
        ActionsMessage::CancelRun => text.literal("Отменить запуск"),
        ActionsMessage::RerunFailed => text.literal("Перезапустить неудачные"),
        ActionsMessage::RerunAll => text.literal("Перезапустить все"),
        ActionsMessage::DownloadTar => text.literal("Скачать как tar.gz"),
        ActionsMessage::Expires => text.literal("истекает"),
        ActionsMessage::RerunJob { name } => {
            text.literal("Перезапустить: ");
            text.argument(name);
        }
        ActionsMessage::Summary { job } => {
            text.literal("Сводка: ");
            text.argument(job);
        }
    }
}
