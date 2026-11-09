//! Russian (`ru`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { Ru:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => text.literal("Выполнение рабочих процессов при отправках и запросах на слияние, а также их секреты и переменные."),
        CardMessage::OrgActions => text.literal("Секреты и переменные для рабочих процессов всех репозиториев этой организации."),
        CardMessage::Collaborators => text.literal("Соавторы"),
        CardMessage::CollaboratorsText => text.literal("Люди и команды с доступом к этому репозиторию и их права."),
        CardMessage::Mirror => text.literal("Зеркало"),
        CardMessage::MirrorText => text.literal("Источник, интервал получения и результат последнего получения."),
        CardMessage::Protection => text.literal("Защита веток"),
        CardMessage::ProtectionText => text.literal("Защитите важные ветки от принудительной отправки, удаления и отправки изменений без запроса на слияние."),
        CardMessage::PushMirrors => text.literal("Push-зеркала"),
        CardMessage::PushMirrorsText => text.literal("Удалённые репозитории, которые получают ветки и теги этого репозитория после каждой отправки."),
        CardMessage::DeployKeys => text.literal("Ключи развёртывания"),
        CardMessage::DeployKeysText => text.literal("SSH-ключи, дающие серверу или заданию CI доступ только к этому репозиторию."),
        CardMessage::Teams => text.literal("Команды"),
        CardMessage::TeamsText => text.literal("Объединяйте участников в группы и давайте каждой группе свой доступ к репозиториям."),
        CardMessage::Transfer => text.literal("Передача владения"),
        CardMessage::TransferText => text.literal("Передайте этот репозиторий другому пользователю или организации. Его URL изменится."),
        CardMessage::Webhooks => text.literal("Вебхуки"),
        CardMessage::RepoWebhooks => text.literal("Уведомляйте другие сервисы об отправке кода, публикации релизов и других событиях."),
        CardMessage::OrgWebhooks => text.literal("Уведомляйте другие сервисы о событиях во всех репозиториях этой организации."),
    }
}
