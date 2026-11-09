//! Russian (`ru`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Ru:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("Раннеры"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal("Раннеры сервера берут задания владельцев, разрешённых администратором, или всех владельцев, если включён параметр ");
            text.argument(setting);
            text.literal(".");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("Токен регистрации создан"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal("Скопируйте его сейчас: больше он показан не будет. На машине, которая должна выполнять задания, выполните команду ниже и вставьте токен, когда она запросит ввод, а затем запустите раннер командой ");
            text.argument(command);
            text.literal(".");
        }
        ActionsRunnersMessage::Token => text.literal("Токен"),
        ActionsRunnersMessage::CopyToken => text.literal("Скопировать токен"),
        ActionsRunnersMessage::Copy => text.literal("Скопировать"),
        ActionsRunnersMessage::Command => text.literal("Команда"),
        ActionsRunnersMessage::CopyCommand => text.literal("Скопировать команду"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("Метки определяют, как выполняются задания: ");
            text.argument(container);
            text.literal(" — в Docker или Podman, ");
            text.argument(host);
            text.literal(" — прямо на машине без изоляции, только для доверенных репозиториев.");
        }
        ActionsRunnersMessage::RepositoryIntro { program } => {
            text.literal("Машины с запущенным ");
            text.argument(program);
            text.literal(", которые выполняют задания этого репозитория.");
        }
        ActionsRunnersMessage::OwnerIntro { program, owner } => {
            text.literal("Машины с запущенным ");
            text.argument(program);
            text.literal(", которые выполняют задания владельца ");
            text.argument(owner);
            text.literal(".");
        }
        ActionsRunnersMessage::AccountIntro { program } => {
            text.literal("Машины с запущенным ");
            text.argument(program);
            text.literal(", которые выполняют задания ваших репозиториев.");
        }
        ActionsRunnersMessage::InstanceIntro { program } => {
            text.literal("Машины с запущенным ");
            text.argument(program);
            text.literal(", которые выполняют задания всех владельцев, которым разрешены раннеры сервера.");
        }
        ActionsRunnersMessage::NoRunners => text.literal("Раннеров пока нет"),
        ActionsRunnersMessage::NoRunnersHint => text.literal("Создайте ниже токен регистрации и зарегистрируйте с его помощью раннер. Задания ждут, пока не появится в сети раннер с их метками."),
        ActionsRunnersMessage::Online => text.literal("в сети"),
        ActionsRunnersMessage::Offline => text.literal("не в сети"),
        ActionsRunnersMessage::Disabled => text.literal("отключён"),
        ActionsRunnersMessage::NeverSeen => text.literal("ещё не подключался"),
        ActionsRunnersMessage::OwnerScope => text.literal("владелец"),
        ActionsRunnersMessage::InstanceScope => text.literal("сервер"),
        ActionsRunnersMessage::Ephemeral => text.literal("одноразовый"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("хост — без изоляции"),
        ActionsRunnersMessage::HostWarning => text.literal("Задания с его метками хоста выполняются от имени пользователя раннера без изоляции"),
        ActionsRunnersMessage::Capacity { count } => {
            text.number(count);
            text.literal(" одновременно");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("был в сети ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("Включить"),
        ActionsRunnersMessage::Disable => text.literal("Отключить"),
        ActionsRunnersMessage::Remove => text.literal("Удалить"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.literal("Удалить ");
            text.argument(name);
        }
        ActionsRunnersMessage::TokensTitle => text.literal("Токены регистрации"),
        ActionsRunnersMessage::TokensIntro => text.literal("Раннер один раз обменивает токен регистрации на собственные учётные данные. Токены одноразовые и истекают, если вы не выберете иное."),
        ActionsRunnersMessage::ExpiresAfter => text.literal("Истекает через"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("Только эти метки"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(необязательно)"),
        ActionsRunnersMessage::Reusable => text.literal("Разрешить регистрацию любого числа раннеров с этим токеном до истечения его срока"),
        ActionsRunnersMessage::NewToken => text.literal("Новый токен регистрации"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("Создан ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("истекает ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("больше не действует"),
        ActionsRunnersMessage::Revoked => text.literal("отозван"),
        ActionsRunnersMessage::Expired => text.literal("истёк"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("без ограничения использований"),
        ActionsRunnersMessage::UsedUp => text.literal("использован"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.literal("осталось ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "использование",
                Plural::Few => "использования",
                _ => "использований",
            });
        }
        ActionsRunnersMessage::Revoke => text.literal("Отозвать"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "час",
                Plural::Few => "часа",
                _ => "часов",
            });
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "день",
                Plural::Few => "дня",
                _ => "дней",
            });
        }
    }
}
