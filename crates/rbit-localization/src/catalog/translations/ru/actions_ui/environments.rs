//! Russian (`ru`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("Окружения"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("Задание, которое указывает окружение через ");
            text.argument(key);
            text.literal(", получает его секреты и переменные, ждёт одобрения одного из его рецензентов, если они заданы, и запускается только из разрешённых им веток. Окружение появляется здесь, когда рабочий процесс впервые на него ссылается.");
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("Окружений нет"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("Добавьте окружение ниже или запустите рабочий процесс, задание которого указывает окружение через ");
            text.argument(key);
            text.literal(".");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("Новое окружение"),
        ActionsEnvironmentMessage::Name => text.literal("Имя"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("Добавить окружение"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "рецензент",
                Plural::Few => "рецензента",
                _ => "рецензентов",
            });
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("защищённые ветки"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("ветки ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("Без защиты"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("Правила защиты"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("Обязательные рецензенты"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("одно имя пользователя на строку"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal("До шести человек, которые могут читать этот репозиторий. Задание в этом окружении ждёт, пока один из них его одобрит; отклонённое задание завершается с ошибкой. Если рецензентов нет, задания выполняются без ревью."),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("Запретить самоодобрение"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => text.literal("Тот, кто запустил выполнение, не может одобрять его развёртывания."),
        ActionsEnvironmentMessage::WaitTimer => text.literal("Таймер ожидания"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal("Сколько минут задание в этом окружении ждёт перед запуском после того, как оно готово и одобрено. 0 — без ожидания; не более 43200 (30 дней)."),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("Ветки и теги для развёртывания"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("Любая ветка или тег"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("Только защищённые ветки"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal("Ветки, для которых есть правило защиты."),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("Выбранные ветки и теги"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("Шаблоны веток и тегов"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("По одному шаблону на строку, как в фильтрах рабочих процессов. Ветки — по имени (");
            text.argument(branch);
            text.literal("), теги — с ");
            text.argument(tags);
            text.literal(" (");
            text.argument(tag);
            text.literal("); ");
            text.argument(exclude);
            text.literal(" исключает.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("Сохранить правила защиты"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("Удалить это окружение"),
        ActionsEnvironmentMessage::DeleteHint => text.literal("Его секреты, переменные и правила тоже будут удалены. Задания, ожидающие его рецензентов, сразу запустятся."),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("Удалить окружение"),
    }
}
