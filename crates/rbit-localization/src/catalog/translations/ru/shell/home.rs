//! Russian (`ru`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::HomeMessage;
use crate::catalog::shell::home::{Feature, Involvement};
use crate::plural::Plural;

entries! { Ru:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("Поиск по публичным репозиториям"),
        HomeMessage::Search => text.literal("Поиск"),
        HomeMessage::ExploreAll => text.literal("Обзор всех репозиториев"),
        HomeMessage::ThisInstance => text.literal("На этом сервере"),
        HomeMessage::RepositoryCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Репозиторий",
                Plural::Few => "Репозитория",
                _ => "Репозиториев",
            });
        }
        HomeMessage::UserCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Пользователь",
                Plural::Few => "Пользователя",
                _ => "Пользователей",
            });
        }
        HomeMessage::OrganizationCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Организация",
                Plural::Few => "Организации",
                _ => "Организаций",
            });
        }
        HomeMessage::RecentlyUpdated => text.literal("Недавно обновлённые"),
        HomeMessage::ViewAll => text.literal("Показать все"),
        HomeMessage::NoPublicRepositories => text.literal("Публичных репозиториев пока нет"),
        HomeMessage::NoPublicRepositoriesText => text.literal("Здесь появятся репозитории, которые станут публичными на этом сервере."),
        HomeMessage::FreshServer => text.literal("На этом сервере ещё нет учётных записей. Первая зарегистрированная учётная запись станет его администратором."),
        HomeMessage::BuiltForSelfHosting => text.literal("Создан для самостоятельного размещения"),
        HomeMessage::FeatureTitle(Feature::Binary) => text.literal("Один быстрый исполняемый файл"),
        HomeMessage::FeatureTitle(Feature::Protocols) => text.literal("Git по HTTPS и SSH"),
        HomeMessage::FeatureTitle(Feature::Teams) => text.literal("Организации и команды"),
        HomeMessage::FeatureTitle(Feature::Releases) => text.literal("Релизы, зеркала и вебхуки"),
        HomeMessage::FeatureTitle(Feature::Security) => text.literal("Безопасность по умолчанию"),
        HomeMessage::FeatureTitle(Feature::Screens) => text.literal("На любом экране"),
        HomeMessage::FeatureText(Feature::Binary) => text.literal("SQLite и встроенное хранилище. Никаких других сервисов устанавливать и запускать не нужно."),
        HomeMessage::FeatureText(Feature::Protocols) => text.literal("Smart HTTP с протоколом v2, встроенный SSH-сервер и ключи развёртывания."),
        HomeMessage::FeatureText(Feature::Teams) => text.literal("Открывайте доступ к репозиториям командам и соавторам — с нужной ролью для каждого."),
        HomeMessage::FeatureText(Feature::Releases) => text.literal("Публикуйте релизы, зеркалируйте вышестоящие проекты и уведомляйте другие сервисы."),
        HomeMessage::FeatureText(Feature::Security) => text.literal("Двухфакторный вход, токены доступа с ограниченными правами и журнал аудита."),
        HomeMessage::FeatureText(Feature::Screens) => text.literal("Адаптивный интерфейс вплоть до экрана телефона и установка в виде приложения."),
        HomeMessage::Dashboard => text.literal("Панель управления"),
        HomeMessage::ViewProfile => text.literal("Открыть профиль"),
        HomeMessage::New => text.literal("Создать"),
        HomeMessage::NewRepository => text.literal("Новый репозиторий"),
        HomeMessage::PersonalDashboards => text.literal("Личные панели"),
        HomeMessage::Issues => text.literal("Задачи"),
        HomeMessage::PullRequests => text.literal("Запросы на слияние"),
        HomeMessage::Repositories => text.literal("Репозитории"),
        HomeMessage::PrivateCount(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.number(count);
            text.literal(match text.plural(count) { Plural::One => " приватный", _ => " приватных" });
        }
        HomeMessage::FindRepository => text.literal("Найти репозиторий…"),
        HomeMessage::FilterRepositories => text.literal("Фильтр репозиториев"),
        HomeMessage::NoRepositories => text.literal("Репозиториев пока нет"),
        HomeMessage::NoRepositoriesText => text.literal("Создайте репозиторий, чтобы начать размещать код на этом сервере."),
        HomeMessage::CreateRepository => text.literal("Создать репозиторий"),
        HomeMessage::NoFilterMatch => text.literal("Нет репозиториев, соответствующих фильтру."),
        HomeMessage::Organizations => text.literal("Организации"),
        HomeMessage::NewOrganization => text.literal("Новая организация"),
        HomeMessage::NotAMember => text.literal("Вы не состоите ни в одной организации."),
        HomeMessage::CreateOne => text.literal("Создайте организацию"),
        HomeMessage::ShareWithTeam => text.literal("для совместной работы над репозиториями с командой."),
        HomeMessage::Explore => text.literal("Обзор"),
        HomeMessage::NothingPublic => text.literal("Публичного пока ничего нет."),
        HomeMessage::YourInvolvement => text.literal("Ваше участие"),
        HomeMessage::InvolvementTab(Involvement::Created) => text.literal("Созданные мной"),
        HomeMessage::InvolvementTab(Involvement::Assigned) => text.literal("Назначенные мне"),
        HomeMessage::InvolvementTab(Involvement::Mentioned) => text.literal("С упоминанием"),
        HomeMessage::InvolvementTab(Involvement::ReviewRequested) => text.literal("Запрошено ревью"),
        HomeMessage::SearchIn(title) => {
            text.literal("Поиск: ");
            text.argument(title);
        }
        HomeMessage::SearchHelp => text.literal("Справка по поиску"),
        HomeMessage::SearchHelpText => text.literal("Используйте is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee или sort:updated-desc. Текст ищется в заголовках и описаниях. В результаты попадают только репозитории, которые вы сейчас можете читать. «С упоминанием» включает известные упоминания в описаниях, комментариях и отправленных ревью, даже после прочтения уведомлений."),
        HomeMessage::Results => text.literal("Результаты"),
        HomeMessage::OpenCount(count) => {
            text.literal("Открыто: ");
            text.number(count);
        }
        HomeMessage::ClosedCount(count) => {
            text.literal("Закрыто: ");
            text.number(count);
        }
        HomeMessage::NoMatchingThreads => text.literal("Ничего не найдено"),
        HomeMessage::NoMatchingThreadsText => text.literal("Попробуйте другой фильтр или другую вкладку."),
        HomeMessage::CommentCount(count) => {
            text.number(u64::from(count));
            text.literal(" ");
            text.literal(match text.plural(u64::from(count)) {
                Plural::One => "комментарий",
                Plural::Few => "комментария",
                _ => "комментариев",
            });
        }
    }
}
