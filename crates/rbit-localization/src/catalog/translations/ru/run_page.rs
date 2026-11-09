//! Russian (`ru`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;
use crate::plural::Plural;

entries! { Ru:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("Этот запуск ждёт одобрения"),
        RunPageMessage::ApprovalExplanation => text.literal("Он пришёл из форка или от пользователя без права записи. Его рабочий процесс может выполнить код, который вы не проверяли, поэтому сначала его должен одобрить пользователь с правом записи. Даже после одобрения он выполняется с токеном только для чтения и без секретов и переменных."),
        RunPageMessage::TrustAuthor => text.literal("Всегда запускать запросы на слияние этого автора без одобрения"),
        RunPageMessage::ApproveAndRun => text.literal("Одобрить и запустить"),
        RunPageMessage::UntrustedNote => text.literal("Этот запуск пришёл из форка или от стороннего автора: у него токен только для чтения и нет секретов и переменных."),
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal("Этот запуск пришёл из форка или от стороннего автора: у него токен только для чтения и нет секретов и переменных. Одобрил: ");
            text.argument(approver);
            text.literal(".");
        }
        RunPageMessage::DeploymentWaiting => text.literal("Ожидает ревью:"),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "выполняет",
                _ => "выполняют",
            });
            text.literal(" развёртывание в окружение ");
            text.argument(environment);
            text.literal("; сначала его должен одобрить рецензент окружения.");
        }
        RunPageMessage::Comment => text.literal("Комментарий"),
        RunPageMessage::Optional => text.literal("(необязательно)"),
        RunPageMessage::ApproveAndDeploy => text.literal("Одобрить и развернуть"),
        RunPageMessage::Reject => text.literal("Отклонить"),
        RunPageMessage::Needs { jobs } => {
            text.literal("зависит от ");
            text.argument(jobs);
        }
    }
}
