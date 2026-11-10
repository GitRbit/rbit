//! Russian (`ru`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;
use crate::plural::Plural;

entries! { Ru:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("Сведения о задании"),
        ActionsJobMessage::Steps => text.literal("Шаги и журналы"),
        ActionsJobMessage::RunsOn => text.literal("выполняется на"),
        ActionsJobMessage::Duration => text.literal("Длительность"),
        ActionsJobMessage::RawLog => text.literal("Полный журнал"),
        ActionsJobMessage::Rerun => text.literal("Перезапустить задание"),
        ActionsJobMessage::NotStarted => text.literal("Ещё не начато"),
        ActionsJobMessage::NotStartedDescription => text.literal("Шаги и их журналы появятся здесь, когда раннер возьмёт это задание."),
        ActionsJobMessage::RetainedWindow => text.literal("Предпросмотр шагов ограничен. Откройте шаг, чтобы увидеть больше строк, или скачайте полный журнал."),
        ActionsJobMessage::Expired => text.literal("Этот журнал удалён по правилам хранения журналов."),
        ActionsJobMessage::NoOutput => text.literal("Нет вывода."),
        ActionsJobMessage::EarlierLines => text.literal("Более ранние строки не показаны."),
        ActionsJobMessage::ShowMore => text.literal("Показать больше строк этого шага"),
        ActionsJobMessage::Or => text.literal(", или"),
        ActionsJobMessage::DownloadAll => text.literal("скачайте полный журнал целиком."),
        ActionsJobMessage::Truncated => text.literal("Задание вывело больше, чем хранит сервер; последующие строки отброшены."),
        ActionsJobMessage::StepNotStarted => text.literal("Этот шаг ещё не начат."),
        ActionsJobMessage::Runner => text.literal("Раннер"),
        ActionsJobMessage::QueuedAt => text.literal("Поставлено в очередь"),
        ActionsJobMessage::StartedAt => text.literal("Начато"),
        ActionsJobMessage::Attempt => text.literal("Попытка"),
        ActionsJobMessage::FailedStep => text.literal("Сбой на шаге"),
        ActionsJobMessage::LogTools => text.literal("Инструменты журнала"),
        ActionsJobMessage::SearchLogs => text.literal("Поиск по журналам"),
        ActionsJobMessage::PreviousMatch => text.literal("Предыдущее совпадение"),
        ActionsJobMessage::NextMatch => text.literal("Следующее совпадение"),
        ActionsJobMessage::ShowTimestamps => text.literal("Показывать время"),
        ActionsJobMessage::ExpandAll => text.literal("Развернуть всё"),
        ActionsJobMessage::CollapseAll => text.literal("Свернуть всё"),
        ActionsJobMessage::DownloadLog => text.literal("Скачать журнал"),
        ActionsJobMessage::LinkCopied => text.literal("Ссылка на строку скопирована"),
        ActionsJobMessage::NoMatches => text.literal("Нет совпадений"),
        ActionsJobMessage::SetUpJob => text.literal("Подготовка задания"),
        ActionsJobMessage::CompleteJob => text.literal("Завершение задания"),
        ActionsJobMessage::Success => text.literal("Успешно"),
        ActionsJobMessage::Failure => text.literal("Ошибка"),
        ActionsJobMessage::Cancelled => text.literal("Отменено"),
        ActionsJobMessage::Skipped => text.literal("Пропущено"),
        ActionsJobMessage::Queued => text.literal("В очереди"),
        ActionsJobMessage::Running => text.literal("Выполняется"),
        ActionsJobMessage::Waiting => text.literal("В ожидании"),
        ActionsJobMessage::Approval => text.literal("Ждёт одобрения"),
        ActionsJobMessage::WorkflowError => text.literal("Ошибка рабочего процесса"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " шаг",
                Plural::Few => " шага",
                _ => " шагов",
            });
        }
    }
}
