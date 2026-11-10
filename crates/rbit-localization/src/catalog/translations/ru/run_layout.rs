//! Russian (`ru`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;
use crate::plural::Plural;

entries! { Ru:
    RunLayoutMessage<'a> => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("Сводка"),
        RunLayoutMessage::Navigation => text.literal("Навигация по запуску"),
        RunLayoutMessage::Usage => text.literal("Использование"),
        RunLayoutMessage::WorkflowFile => text.literal("Файл рабочего процесса"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " задание",
                Plural::Few => " задания",
                _ => " заданий",
            });
        }
        RunLayoutMessage::Graph => text.literal("Граф заданий"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("ещё ");
            text.number(count);
        }
        RunLayoutMessage::Annotations => text.literal("Аннотации"),
        RunLayoutMessage::Errors { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ошибка",
                Plural::Few => " ошибки",
                _ => " ошибок",
            });
        }
        RunLayoutMessage::Warnings { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " предупреждение",
                Plural::Few => " предупреждения",
                _ => " предупреждений",
            });
        }
        RunLayoutMessage::Notices { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " уведомление",
                Plural::Few => " уведомления",
                _ => " уведомлений",
            });
        }
        RunLayoutMessage::ErrorLevel => text.literal("Ошибка"),
        RunLayoutMessage::WarningLevel => text.literal("Предупреждение"),
        RunLayoutMessage::NoticeLevel => text.literal("Уведомление"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("Шаг ");
            text.argument(step);
            text.literal(" завершился с ошибкой.");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("И ещё ");
            text.number(count);
            text.literal(" в журналах заданий.");
        }
        RunLayoutMessage::TriggeredVia => text.literal("Событие"),
        RunLayoutMessage::Status => text.literal("Статус"),
        RunLayoutMessage::TotalDuration => text.literal("Общая длительность"),
        RunLayoutMessage::RerunBy { name } => {
            text.literal("Перезапустил(а) ");
            text.argument(name);
        }
        RunLayoutMessage::Inputs => text.literal("Входные данные"),
        RunLayoutMessage::Waited { time } => {
            text.literal("ожидание ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("на ");
            text.argument(runner);
        }
    }
}
