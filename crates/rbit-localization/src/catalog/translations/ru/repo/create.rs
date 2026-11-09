//! Russian (`ru`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("Новый репозиторий"),
        RepoCreateLabel::RepositoryType => text.literal("Тип репозитория"),
        RepoCreateLabel::Empty => text.literal("Пустой"),
        RepoCreateLabel::Mirror => text.literal("Зеркало"),
        RepoCreateLabel::Import => text.literal("Импорт"),
        RepoCreateLabel::NewLead => text.literal("Создайте пустой репозиторий, импортируйте проект или поддерживайте зеркало в синхронизации с источником."),
        RepoCreateLabel::Owner => text.literal("Владелец"),
        RepoCreateLabel::Name => text.literal("Имя"),
        RepoCreateLabel::Description => text.literal("Описание"),
        RepoCreateLabel::Optional => text.literal("необязательно"),
        RepoCreateLabel::Visibility => text.literal("Видимость"),
        RepoCreateLabel::Public => text.literal("Публичный"),
        RepoCreateLabel::Private => text.literal("Приватный"),
        RepoCreateLabel::PublicRepository => text.literal("Любой может просматривать и клонировать этот репозиторий."),
        RepoCreateLabel::PublicMirror => text.literal("Любой может просматривать и клонировать зеркало."),
        RepoCreateLabel::PublicImport => text.literal("Любой может просматривать и клонировать репозиторий."),
        RepoCreateLabel::PrivateHint => text.literal("Виден только владельцу. Для организации — её участникам."),
        RepoCreateLabel::Cancel => text.literal("Отмена"),
        RepoCreateLabel::CreateRepository => text.literal("Создать репозиторий"),
        RepoCreateLabel::MirrorLead => text.literal("Зеркало по расписанию получает ветки и теги публичного репозитория, размещённого в другом месте. Здесь оно доступно только для чтения: изменения приходят из источника."),
        RepoCreateLabel::MirrorsDisabled => text.literal("Зеркала отключены"),
        RepoCreateLabel::MirrorsDisabledText => text.literal("Администратор этого сервера отключил зеркала."),
        RepoCreateLabel::UpstreamUrl => text.literal("URL источника"),
        RepoCreateLabel::UrlHintStart => text.literal("Адрес"),
        RepoCreateLabel::UrlHintOr => text.literal("или"),
        RepoCreateLabel::UrlHintEnd => text.literal("публичного репозитория."),
        RepoCreateLabel::FromTheUrl => text.literal("из URL"),
        RepoCreateLabel::FetchEvery => text.literal("Получать каждые"),
        RepoCreateLabel::Minutes => text.literal("минут"),
        RepoCreateLabel::CreateMirror => text.literal("Создать зеркало"),
        RepoCreateLabel::MirrorARepository => text.literal("Зеркалирование репозитория"),
        RepoCreateLabel::ImportLead => text.literal("Копирует репозиторий из GitHub или Gitea вместе с историей и, при желании, задачами, запросами на слияние, метками, этапами, релизами и файлами Git LFS. Номера задач и запросов на слияние сохраняются."),
        RepoCreateLabel::ImportingDisabled => text.literal("Импорт отключён"),
        RepoCreateLabel::ImportingDisabledText => text.literal("Администратор этого сервера отключил импорт."),
        RepoCreateLabel::ImportFrom => text.literal("Импорт из"),
        RepoCreateLabel::ServerAddress => text.literal("Адрес сервера"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("не нужен для github.com"),
        RepoCreateLabel::Repository => text.literal("Репозиторий"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("владелец/имя или URL"),
        RepoCreateLabel::AccessToken => text.literal("Токен доступа"),
        RepoCreateLabel::OptionalForPublic => text.literal("необязателен для публичных репозиториев"),
        RepoCreateLabel::TokenHint => text.literal("Токен с правом чтения репозитория повышает лимиты запросов источника и нужен для приватных репозиториев. Он используется только во время импорта и никогда не сохраняется."),
        RepoCreateLabel::AsOnTheSource => text.literal("как в источнике"),
        RepoCreateLabel::BringAlong => text.literal("Перенести также"),
        RepoCreateLabel::Wiki => text.literal("Вики"),
        RepoCreateLabel::WikiNotSupported => text.literal("Не поддерживается: на этом сервере нет вики."),
        RepoCreateLabel::StartImport => text.literal("Начать импорт"),
        RepoCreateLabel::ImportARepository => text.literal("Импорт репозитория"),
        RepoCreateLabel::ImportTitle => text.literal("Импорт"),
        RepoCreateLabel::FailedNote => text.literal("Всё, что было импортировано до этого момента, есть в репозитории."),
        RepoCreateLabel::NotReached => text.literal("Не выполнено"),
        RepoCreateLabel::Started => text.literal("Начато"),
        RepoCreateLabel::WaitingForSlot => text.literal("Ожидание свободного слота"),
        RepoCreateLabel::Ended => text.literal("Завершено"),
        RepoCreateLabel::UpdatesByItself => text.literal("Эта страница обновляется автоматически."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("Пропущено или не завершено"),
        RepoCreateLabel::GoToRepository => text.literal("Перейти в репозиторий"),
        RepoCreateLabel::OpensAutomatically => text.literal("Репозиторий откроется автоматически, когда завершится первое получение зеркала."),
        RepoCreateLabel::FetchInProgress => text.literal("Идёт получение зеркала"),
        RepoCreateLabel::TransferSizeNote => text.literal("Объём передачи определяется источником. На этой странице показан текущий этап работы."),
        RepoCreateLabel::RefreshStatus => text.literal("Обновить статус"),
        RepoCreateLabel::SettingsAndRetry => text.literal("Настройки зеркала и повтор"),
        RepoCreateLabel::NoScriptHint => text.literal("Нажмите «Обновить статус», чтобы проверить ход работы, и откройте репозиторий, когда он будет готов."),
        RepoCreateLabel::Connecting => text.literal("Подключение к исходному репозиторию."),
        RepoCreateLabel::Fetching => text.literal("Загрузка веток, тегов и объектов Git. Для больших репозиториев это может занять несколько минут."),
        RepoCreateLabel::Finalizing => text.literal("Подготовка содержимого репозитория и ветки по умолчанию."),
        RepoCreateLabel::FetchFailedRetry => text.literal("Не удалось получить зеркало. Администратор репозитория может повторить попытку в настройках зеркала."),
        RepoCreateLabel::Complete => text.literal("Зеркалирование завершено. Открываем репозиторий…"),
        RepoCreateLabel::Queued => text.literal("Ожидание свободного обработчика зеркал."),
        RepoCreateLabel::Stopped => text.literal("Зеркалирование остановлено. Открываем репозиторий…"),
        RepoCreateLabel::Mirroring => text.literal("Зеркалирование"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("Не менее ");
            text.number(minutes);
            text.literal(" ");
            text.literal(match text.plural(minutes) {
                Plural::One => "минуты",
                _ => "минут",
            });
            text.literal(".");
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("Из ");
            text.argument(source);
            text.literal(":");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("и ещё ");
            text.number(count);
            text.literal(" (см. журнал сервера)");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.literal("Зеркалирование: ");
            text.argument(name);
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("Не удалось получить зеркало: ");
            text.argument(error);
        }
    }
}
