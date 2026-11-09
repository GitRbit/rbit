//! Russian (`ru`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};
use crate::plural::Plural;

entries! { Ru:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("Настройки"),
        RepoSettingsLabel::AdminsOf => text.literal("Только администраторы"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("могут видеть эту страницу."),
        RepoSettingsLabel::SettingsSaved => text.literal("Настройки сохранены."),
        RepoSettingsLabel::General => text.literal("Общие"),
        RepoSettingsLabel::Description => text.literal("Описание"),
        RepoSettingsLabel::Optional => text.literal("необязательно"),
        RepoSettingsLabel::Visibility => text.literal("Видимость"),
        RepoSettingsLabel::Public => text.literal("Публичный"),
        RepoSettingsLabel::PublicHint => text.literal("Любой может просматривать и клонировать этот репозиторий."),
        RepoSettingsLabel::Private => text.literal("Приватный"),
        RepoSettingsLabel::PrivateHint => text.literal("Просматривать и клонировать его могут только люди с доступом."),
        RepoSettingsLabel::DefaultBranch => text.literal("Ветка по умолчанию"),
        RepoSettingsLabel::PushABranch => text.literal("Отправьте ветку, чтобы выбрать ветку по умолчанию."),
        RepoSettingsLabel::DefaultBranchHint => text.literal("Показывается первой при просмотре, и её получает git clone."),
        RepoSettingsLabel::SaveChanges => text.literal("Сохранить изменения"),
        RepoSettingsLabel::RepositoryName => text.literal("Имя репозитория"),
        RepoSettingsLabel::Name => text.literal("Имя"),
        RepoSettingsLabel::RenameHint => text.literal("Ссылки и URL для клонирования со старым именем перестанут работать."),
        RepoSettingsLabel::Rename => text.literal("Переименовать"),
        RepoSettingsLabel::DangerZone => text.literal("Опасная зона"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("Разархивировать этот репозиторий"),
        RepoSettingsLabel::UnarchiveHint => text.literal("Он снова будет принимать отправки, задачи, запросы на слияние, комментарии и релизы, а его зеркала возобновят работу."),
        RepoSettingsLabel::Unarchive => text.literal("Разархивировать репозиторий"),
        RepoSettingsLabel::ArchiveTitle => text.literal("Архивировать этот репозиторий"),
        RepoSettingsLabel::ArchiveHint => text.literal("Он станет доступен только для чтения: его по-прежнему можно клонировать, форкнуть и отметить звездой, но никто не сможет отправлять изменения, выполнять слияние или изменять его задачи, запросы на слияние и релизы, а его зеркала приостановятся. Разархивировать его можно в любой момент."),
        RepoSettingsLabel::Archive => text.literal("Архивировать репозиторий"),
        RepoSettingsLabel::DeleteTitle => text.literal("Удалить этот репозиторий"),
        RepoSettingsLabel::DeleteHint => text.literal("Его файлы, история и звёзды будут удалены навсегда. Это действие нельзя отменить."),
        RepoSettingsLabel::Type => text.literal("Введите"),
        RepoSettingsLabel::ToConfirm => text.literal("для подтверждения"),
        RepoSettingsLabel::DeleteRepository => text.literal("Удалить репозиторий"),
        RepoSettingsLabel::DeployKeys => text.literal("Ключи развёртывания"),
        RepoSettingsLabel::DeployKeysHint => text.literal("Ключи, дающие серверу или заданию CI доступ по SSH только к этому репозиторию."),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal("Ключ развёртывания даёт серверу или заданию CI доступ по SSH только к репозиторию"),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal("и только для чтения, если вы не разрешите ему отправку изменений."),
        RepoSettingsLabel::NewDeployKey => text.literal("Новый ключ развёртывания"),
        RepoSettingsLabel::Title => text.literal("Название"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("например, Продакшен-сервер"),
        RepoSettingsLabel::TitleHint => text.literal("По умолчанию — комментарий ключа."),
        RepoSettingsLabel::Key => text.literal("Ключ"),
        RepoSettingsLabel::KeyHint => text.literal("Ключ может принадлежать только одному репозиторию или одной учётной записи; создайте отдельный ключ для каждого."),
        RepoSettingsLabel::AllowWrite => text.literal("Разрешить запись"),
        RepoSettingsLabel::AllowWriteHint => text.literal("Ключ сможет не только клонировать этот репозиторий, но и отправлять в него изменения."),
        RepoSettingsLabel::AddDeployKey => text.literal("Добавить ключ развёртывания"),
        RepoSettingsLabel::Keys => text.literal("Ключи"),
        RepoSettingsLabel::NoDeployKeys => text.literal("Ключей развёртывания нет"),
        RepoSettingsLabel::NoDeployKeysText => text.literal("Добавьте ключ, чтобы сервер или задание CI могли клонировать этот репозиторий по SSH."),
        RepoSettingsLabel::Delete => text.literal("Удалить"),
        RepoSettingsLabel::Mirror => text.literal("Зеркало"),
        RepoSettingsLabel::MirrorHint => text.literal("Источник, интервал получения и результат последнего получения."),
        RepoSettingsLabel::MirrorLead => text.literal("Этот репозиторий получает ветки и теги из источника и не принимает отправки."),
        RepoSettingsLabel::Upstream => text.literal("Источник"),
        RepoSettingsLabel::LastFetch => text.literal("Последнее получение"),
        RepoSettingsLabel::Failed => text.literal("Ошибка"),
        RepoSettingsLabel::Ok => text.literal("Успешно"),
        RepoSettingsLabel::NotYet => text.literal("Ещё не выполнялось"),
        RepoSettingsLabel::NextFetch => text.literal("Следующее получение"),
        RepoSettingsLabel::Error => text.literal("Ошибка"),
        RepoSettingsLabel::FetchNow => text.literal("Получить сейчас"),
        RepoSettingsLabel::FetchEvery => text.literal("Получать каждые"),
        RepoSettingsLabel::Minutes => text.literal("минут"),
        RepoSettingsLabel::Save => text.literal("Сохранить"),
        RepoSettingsLabel::Saved => text.literal("Сохранено."),
        RepoSettingsLabel::FetchQueued => text.literal("Получение поставлено в очередь; обновите страницу чуть позже, чтобы увидеть результат."),
        RepoSettingsLabel::PushQueued => text.literal("Отправка поставлена в очередь; обновите страницу чуть позже, чтобы увидеть результат."),
        RepoSettingsLabel::StopMirroring => text.literal("Остановить зеркалирование"),
        RepoSettingsLabel::StopHint => text.literal("Репозиторий сохранит содержимое, перестанет получать изменения и будет принимать отправки, как любой другой."),
        RepoSettingsLabel::PushMirrors => text.literal("Push-зеркала"),
        RepoSettingsLabel::PushMirrorsHint => text.literal("Удалённые репозитории, которые получают ветки и теги этого репозитория после каждой отправки."),
        RepoSettingsLabel::PushMirrorsLead => text.literal("Удалённые репозитории, которые получают ветки и теги этого репозитория. Каждая отправка перезаписывает ветки и теги удалённого репозитория и удаляет те, которых здесь больше нет."),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("Push-зеркала отключены"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal("Администратор этого сервера отключил push-зеркала."),
        RepoSettingsLabel::NoPushMirrors => text.literal("Push-зеркал пока нет"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal("Добавьте удалённый репозиторий ниже, чтобы хранить копию этого репозитория в другом месте."),
        RepoSettingsLabel::Paused => text.literal("Приостановлено"),
        RepoSettingsLabel::OnPush => text.literal("При отправке"),
        RepoSettingsLabel::LastPush => text.literal("Последняя отправка"),
        RepoSettingsLabel::NotPushedYet => text.literal("Ещё не отправлялось"),
        RepoSettingsLabel::As => text.literal("от имени"),
        RepoSettingsLabel::SecretConfigured => text.literal("секрет задан"),
        RepoSettingsLabel::PushNow => text.literal("Отправить сейчас"),
        RepoSettingsLabel::Edit => text.literal("Изменить"),
        RepoSettingsLabel::Remove => text.literal("Удалить"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("Добавление push-зеркала"),
        RepoSettingsLabel::AddPushMirror => text.literal("Добавить push-зеркало"),
        RepoSettingsLabel::PushMirrorAdded => text.literal("Push-зеркало добавлено. Первая отправка уже идёт; обновите страницу чуть позже, чтобы увидеть результат."),
        RepoSettingsLabel::PushMirrorRemoved => text.literal("Push-зеркало удалено. Удалённый репозиторий сохранит то, что у него есть."),
        RepoSettingsLabel::EditPushMirror => text.literal("Редактирование push-зеркала"),
        RepoSettingsLabel::PushMirror => text.literal("Push-зеркало"),
        RepoSettingsLabel::Enabled => text.literal("Включено"),
        RepoSettingsLabel::EnabledHint => text.literal("Приостановленное зеркало сохраняет настройки, но в него ничего не отправляется."),
        RepoSettingsLabel::Cancel => text.literal("Отмена"),
        RepoSettingsLabel::RemoteUrl => text.literal("URL удалённого репозитория"),
        RepoSettingsLabel::UrlHintStart => text.literal("Адрес"),
        RepoSettingsLabel::UrlHintOr => text.literal("или"),
        RepoSettingsLabel::UrlHintEnd => text.literal("без имени пользователя и пароля."),
        RepoSettingsLabel::UserName => text.literal("Имя пользователя"),
        RepoSettingsLabel::PasswordOrToken => text.literal("Пароль или токен"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("Задан — оставьте пустым, чтобы сохранить"),
        RepoSettingsLabel::SecretHint => text.literal("Передаётся удалённому репозиторию через базовую аутентификацию HTTP. Повторно показать его нельзя."),
        RepoSettingsLabel::SecretHintKept => text.literal("Передаётся удалённому репозиторию через базовую аутентификацию HTTP. Повторно показать его нельзя, и его нужно ввести заново, если удалённый репозиторий переедет на другой хост."),
        RepoSettingsLabel::PushAfterPush => text.literal("Отправлять после каждой отправки"),
        RepoSettingsLabel::PushAfterPushHint => text.literal("Через несколько секунд после изменения этого репозитория. Кроме того, каждое зеркало отправляется по расписанию."),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("Не менее ");
            text.number(minutes);
            text.literal(" ");
            text.literal(match text.plural(minutes) {
                Plural::One => "минуты",
                _ => "минут",
            });
            text.literal(".");
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("Каждое зеркало отправляется не реже чем раз в ");
            text.number(minutes);
            text.literal(" ");
            text.literal(match text.plural(minutes) {
                Plural::One => "минуту",
                Plural::Few => "минуты",
                _ => "минут",
            });
            text.literal(".");
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.literal("Удалить ");
            text.argument(title);
        }
    }
}
