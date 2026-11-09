//! Russian (`ru`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { Ru:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("отправка"),
        OverviewControlMessage::PullRequest => text.literal("запрос на слияние"),
        OverviewControlMessage::Manual => text.literal("вручную"),
        OverviewControlMessage::Schedule => text.literal("по расписанию"),
        OverviewControlMessage::WorkflowRun => text.literal("после рабочего процесса"),
        OverviewControlMessage::Dispatch => text.literal("по вызову"),
        OverviewControlMessage::Success => text.literal("Успешно"),
        OverviewControlMessage::Failure => text.literal("Ошибка"),
        OverviewControlMessage::Cancelled => text.literal("Отменено"),
        OverviewControlMessage::Skipped => text.literal("Пропущено"),
        OverviewControlMessage::Queued => text.literal("В очереди"),
        OverviewControlMessage::Running => text.literal("Выполняется"),
        OverviewControlMessage::Waiting => text.literal("В ожидании"),
        OverviewControlMessage::Approval => text.literal("Ждёт одобрения"),
        OverviewControlMessage::RunWorkflow => text.literal("Запустить рабочий процесс"),
        OverviewControlMessage::WorkflowFrom => text.literal("Взять рабочий процесс из"),
        OverviewControlMessage::Change => text.literal("Изменить"),
    }
}
