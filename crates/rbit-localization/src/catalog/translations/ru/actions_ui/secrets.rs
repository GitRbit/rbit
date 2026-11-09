//! Russian (`ru`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("Секреты"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("Рабочие процессы читают их как ");
            text.argument(expression);
            text.literal(". Значения зашифрованы, и их нельзя показать снова — только заменить. Запуски из форков и от авторов без права записи никогда их не получают; любой, кто может отправлять изменения, может прочитать их через рабочий процесс.");
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("Секреты нельзя сохранять и использовать: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("Секретов нет"),
        ActionsSecretsMessage::NoSecretsHint => text.literal("Добавьте секрет для токенов и ключей, которые нужны вашим рабочим процессам."),
        ActionsSecretsMessage::CannotDecrypt => text.literal("Не удаётся расшифровать — задайте заново"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("Обновлено ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("Все репозитории"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "выбранный репозиторий",
                Plural::Few => "выбранных репозитория",
                _ => "выбранных репозиториев",
            });
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("Удалить секрет ");
            text.argument(name);
        }
        ActionsSecretsMessage::Delete => text.literal("Удалить"),
        ActionsSecretsMessage::ReplaceValue => text.literal("Заменить значение"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.literal("Новое значение ");
            text.argument(name);
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("Заменить секрет"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("Выбрать репозитории"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("Репозитории, рабочие процессы которых получают ");
            text.argument(name);
            text.literal(", без изменения его значения.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("Сохранить репозитории"),
        ActionsSecretsMessage::NewSecret => text.literal("Новый секрет"),
        ActionsSecretsMessage::Name => text.literal("Имя"),
        ActionsSecretsMessage::SecretNameHint => text.literal("Буквы, цифры и подчёркивания; имя не может начинаться с цифры или с GITHUB_, GITEA_, FORGEJO_ или RBIT_. Имена хранятся в верхнем регистре; секрет с существующим именем заменяется."),
        ActionsSecretsMessage::Value => text.literal("Значение"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("Не более ");
            text.number(kib);
            text.literal(" КиБ. Повторно показать значение нельзя.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("Добавить секрет"),
        ActionsSecretsMessage::VariablesTitle => text.literal("Переменные"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("Рабочие процессы читают их как ");
            text.argument(expression);
            text.literal(". Они не секретны: любой, кто может изменить рабочий процесс, может их вывести. Запуски из форков и от авторов без права записи никогда их не получают.");
        }
        ActionsSecretsMessage::NoVariables => text.literal("Переменных нет"),
        ActionsSecretsMessage::NoVariablesHint => text.literal("Добавьте переменную для общих настроек рабочих процессов, например региона или версии."),
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("Удалить переменную ");
            text.argument(name);
        }
        ActionsSecretsMessage::Edit => text.literal("Изменить"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.literal("Значение ");
            text.argument(name);
        }
        ActionsSecretsMessage::SaveVariable => text.literal("Сохранить переменную"),
        ActionsSecretsMessage::NewVariable => text.literal("Новая переменная"),
        ActionsSecretsMessage::VariableNameHint => text.literal("Те же правила, что и для имён секретов; переменная с существующим именем заменяется."),
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("Не более ");
            text.number(kib);
            text.literal(" КиБ.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("Добавить переменную"),
    }
}
