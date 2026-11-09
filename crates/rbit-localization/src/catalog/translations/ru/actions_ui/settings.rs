//! Russian (`ru`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.literal("Рабочие процессы из ");
            text.argument(directory);
            text.literal(" выполняются на раннерах, когда в ");
            text.argument(repository);
            text.literal(" происходят их события.");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("Секреты и переменные для рабочих процессов репозиториев организации ");
            text.argument(organization);
            text.literal(". Собственный секрет или переменная репозитория с тем же именем имеет приоритет.");
        }
        ActionsSettingsMessage::AccountIntro => text.literal("Секреты и переменные для рабочих процессов ваших репозиториев. Собственный секрет или переменная репозитория с тем же именем имеет приоритет."),
        ActionsSettingsMessage::General => text.literal("Общие"),
        ActionsSettingsMessage::RunWorkflows => text.literal("Выполнять рабочие процессы в этом репозитории"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal("Если выключено, новые запуски не начинаются, а вкладка Actions скрыта. Прежние запуски и их журналы сохраняются."),
        ActionsSettingsMessage::DefaultPermission => text.literal("Права токена рабочего процесса по умолчанию"),
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("Ключ ");
            text.argument(key);
            text.literal(" рабочего процесса может только понизить их. Запуски из форков и от сторонних авторов всегда получают токен только для чтения.");
        }
        ActionsSettingsMessage::PermissionRead => text.literal("Чтение"),
        ActionsSettingsMessage::PermissionReadHint => text.literal("Рабочие процессы могут читать содержимое репозитория, задачи и запросы на слияние."),
        ActionsSettingsMessage::PermissionWrite => text.literal("Чтение и запись"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal("Рабочие процессы также могут отправлять изменения и комментировать задачи и запросы на слияние. Защита веток по-прежнему действует."),
        ActionsSettingsMessage::SaveChanges => text.literal("Сохранить изменения"),
        ActionsSettingsMessage::RepositoryRunnersHint => text.literal("Раннеры этого репозитория, унаследованные им раннеры и токены регистрации."),
        ActionsSettingsMessage::OrganizationRunnersHint => text.literal("Раннеры для всех репозиториев организации и токены регистрации."),
        ActionsSettingsMessage::AccountRunnersHint => text.literal("Раннеры для всех репозиториев вашей учётной записи и токены регистрации."),
        ActionsSettingsMessage::EnvironmentsHint => text.literal("Окружения развёртывания: обязательные рецензенты, разрешённые ветки, собственные секреты и переменные."),
        ActionsSettingsMessage::TrustedTitle => text.literal("Доверенные авторы запросов на слияние"),
        ActionsSettingsMessage::TrustedIntro => text.literal("Их запросы на слияние из форков запускаются без ожидания одобрения, но по-прежнему с токеном только для чтения и без секретов и переменных. Авторы добавляются при одобрении запуска с отметкой «всегда запускать запросы на слияние этого автора»."),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("Доверенных авторов нет"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal("Запросы на слияние из форков и от людей без права записи ждут, пока пользователь с правом записи одобрит каждый запуск."),
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("Доверие с ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("Доверие с ");
            text.argument(since);
            text.literal(", одобрил: ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.literal("Перестать доверять ");
            text.argument(name);
        }
        ActionsSettingsMessage::Remove => text.literal("Удалить"),
        ActionsSettingsMessage::Saved => text.literal("Настройки сохранены."),
        ActionsSettingsMessage::SecretSaved => text.literal("Секрет сохранён."),
        ActionsSettingsMessage::SecretDeleted => text.literal("Секрет удалён."),
        ActionsSettingsMessage::VariableSaved => text.literal("Переменная сохранена."),
        ActionsSettingsMessage::VariableDeleted => text.literal("Переменная удалена."),
        ActionsSettingsMessage::Untrusted => text.literal("Запросы на слияние этого автора снова требуют одобрения."),
        ActionsSettingsMessage::RepositoryAccess => text.literal("Доступ к репозиториям"),
        ActionsSettingsMessage::AllRepositories => text.literal("Все репозитории"),
        ActionsSettingsMessage::AllRepositoriesHint => text.literal("Все репозитории этого владельца, включая созданные позже."),
        ActionsSettingsMessage::SelectedRepositories => text.literal("Выбранные репозитории"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("Только репозитории, отмеченные ниже."),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("У этого владельца пока нет репозиториев."),
    }
}
