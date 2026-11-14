//! Russian (`ru`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;
use crate::plural::Plural;

entries! { Ru:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("Рабочие процессы"),
        ActionsOverviewMessage::AllWorkflows => text.literal("Все рабочие процессы"),
        ActionsOverviewMessage::AllRuns => text.literal("Все запуски рабочих процессов"),
        ActionsOverviewMessage::Filters => text.literal("Фильтры запусков"),
        ActionsOverviewMessage::Branch => text.literal("Ветка"),
        ActionsOverviewMessage::Actor => text.literal("Пользователь"),
        ActionsOverviewMessage::Event => text.literal("Событие"),
        ActionsOverviewMessage::Status => text.literal("Статус"),
        ActionsOverviewMessage::AnyEvent => text.literal("Любое событие"),
        ActionsOverviewMessage::AnyStatus => text.literal("Любой статус"),
        ActionsOverviewMessage::Filter => text.literal("Фильтровать"),
        ActionsOverviewMessage::Clear => text.literal("Сбросить"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("Включить рабочий процесс"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("Отключить рабочий процесс"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal("Этот рабочий процесс отключён: пока его не включат, он не запускается ни по какому событию."),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("Подходящих запусков нет"),
        ActionsOverviewMessage::NoMatchingRunsDescription => text.literal("Ни один запуск не соответствует этим фильтрам."),
        ActionsOverviewMessage::NoRunsYet => text.literal("Запусков пока нет"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal("Этот рабочий процесс выполняется, когда его запускают вручную: воспользуйтесь кнопкой «Запустить рабочий процесс» выше."),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("Запусков рабочих процессов пока нет"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal("Добавьте файл рабочего процесса в .rbit/workflows и отправьте его: запуски появятся здесь, когда их возьмут раннеры."),
        ActionsOverviewMessage::Pagination => text.literal("Постраничная навигация"),
        ActionsOverviewMessage::OlderRuns => text.literal("Более ранние запуски"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " запуск на этой странице",
                Plural::Few => " запуска на этой странице",
                _ => " запусков на этой странице",
            });
        }
        ActionsOverviewMessage::JobsFinished { done, total } => {
            text.literal("Завершено заданий: ");
            text.number(done);
            text.literal(" из ");
            text.number(total);
        }
        ActionsOverviewMessage::JobsRunning { count } => {
            text.literal("Выполняется заданий: ");
            text.number(count);
        }
        ActionsOverviewMessage::WaitingForRunner => text.literal("Ожидание раннера"),
    }
}
