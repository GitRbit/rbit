//! Russian (`ru`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};
use crate::plural::Plural;

entries! { Ru:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("Администрирование сайта"),
        AdminMessage::Administration => text.literal("Администрирование"),
        AdminMessage::Dashboard => text.literal("Панель управления"),
        AdminMessage::Users => text.literal("Пользователи"),
        AdminMessage::Organizations => text.literal("Организации"),
        AdminMessage::Repositories => text.literal("Репозитории"),
        AdminMessage::Settings => text.literal("Настройки"),
        AdminMessage::AuditLog => text.literal("Журнал аудита"),
        AdminMessage::Configuration => text.literal("Конфигурация"),
        AdminMessage::Runners => text.literal("Раннеры"),
        AdminMessage::Search => text.literal("Поиск"),
        AdminMessage::Pagination => text.literal("Нумерация страниц"),
        AdminMessage::Previous => text.literal("Назад"),
        AdminMessage::Next => text.literal("Далее"),
        AdminMessage::NoResults => text.literal("Ничего не найдено"),
        AdminMessage::Cancel => text.literal("Отмена"),
        AdminMessage::Actions => text.literal("Действия"),
        AdminMessage::Private => text.literal("Приватный"),
        AdminMessage::Archived => text.literal("В архиве"),
        AdminMessage::Delete => text.literal("Удалить"),
        AdminMessage::TypeToConfirmLead => text.literal("Введите"),
        AdminMessage::TypeToConfirmTail => text.literal("для подтверждения"),
        AdminMessage::NewPassword => text.literal("Новый пароль"),
        AdminMessage::ConfirmPassword => text.literal("Повторите новый пароль"),
        AdminMessage::Unavailable => text.literal("недоступно"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("Обзор этого сервера."),
        DashboardMessage::RestartServer => text.literal("Перезапустить сервер"),
        DashboardMessage::RestartUnavailableHint => text.literal("Перезапуск недоступен в среде, где размещён этот сервер"),
        DashboardMessage::SoftwareUpdates => text.literal("Обновления ПО"),
        DashboardMessage::ConfigureUpdates => text.literal("Настроить проверку обновлений и раскрытие версии"),
        DashboardMessage::LiveMonitoring => text.literal("Мониторинг в реальном времени"),
        DashboardMessage::MonitoringIntro => text.literal("Трафик, время ответа и активность Git за последние пять минут."),
        DashboardMessage::Snapshot => text.literal("Снимок"),
        DashboardMessage::Pause => text.literal("Пауза"),
        DashboardMessage::NeedsJavaScript => text.literal("Для обновления в реальном времени нужен JavaScript. Перезагрузите страницу, чтобы получить свежий снимок."),
        DashboardMessage::System => text.literal("Система"),
        DashboardMessage::InstanceSettings => text.literal("Настройки сервера"),
        DashboardMessage::Edit => text.literal("Изменить"),
        DashboardMessage::Users => text.literal("Пользователи"),
        DashboardMessage::Organizations => text.literal("Организации"),
        DashboardMessage::SharedNamespaces => text.literal("Общие пространства имён"),
        DashboardMessage::Repositories => text.literal("Репозитории"),
        DashboardMessage::AccessTokens => text.literal("Токены доступа"),
        DashboardMessage::AcrossAllUsers => text.literal("У всех пользователей"),
        DashboardMessage::Uptime => text.literal("Время работы"),
        DashboardMessage::DataDirectory => text.literal("Каталог данных"),
        DashboardMessage::DiskUsage => text.literal("Использование диска"),
        DashboardMessage::SiteName => text.literal("Название сайта"),
        DashboardMessage::Registration => text.literal("Регистрация"),
        DashboardMessage::RegistrationOpen => text.literal("Открыта для всех"),
        DashboardMessage::RegistrationClosed => text.literal("Закрыта (учётные записи создают администраторы)"),
        DashboardMessage::OrgPolicy => text.literal("Организации"),
        DashboardMessage::AnyUserCreates => text.literal("Может создавать любой пользователь"),
        DashboardMessage::AdminsCreate => text.literal("Могут создавать только администраторы"),
        DashboardMessage::NewRepositories => text.literal("Новые репозитории"),
        DashboardMessage::PublicByDefault => text.literal("Публичные по умолчанию"),
        DashboardMessage::PrivateByDefault => text.literal("Приватные по умолчанию"),
        DashboardMessage::EmailDigests => text.literal("Сводки по почте"),
        DashboardMessage::Restarting => text.literal("Перезапуск сервера"),
        DashboardMessage::RestartRequested => text.literal("Перезапуск запрошен"),
        DashboardMessage::RestartingText => text.literal("Сервер завершает активные HTTP-запросы, останавливает SSH и фоновые задачи и закрывает хранилище перед перезапуском. Завершение долгих передач может занять некоторое время."),
        DashboardMessage::Waiting => text.literal("Ожидание возврата сервера…"),
        DashboardMessage::ReturnToDashboard => text.literal("Вернуться на панель управления"),
        DashboardMessage::WaitNoScript => text.literal("Подождите немного, затем вернитесь на панель управления, чтобы переподключиться."),
        DashboardMessage::RestartQuestion => text.literal("Перезапустить это приложение rbit?"),
        DashboardMessage::RestartText => text.literal("Сайт ненадолго станет недоступен, пока приложение завершает работу и снова запускается с сохранённой конфигурацией. HTTP-запросам дадут завершиться; SSH-подключения будут закрыты. Сохранённые сеансы пользователей останутся."),
        DashboardMessage::RestartAudited => text.literal("Это действие записывается в журнал аудита администрирования."),
        DashboardMessage::RestartUnavailable => text.literal("Перезапуск недоступен"),
        DashboardMessage::RestartUnsupported => text.literal("Среда, в которой размещён этот сервер, не поддерживает перезапуск приложения. Перезапустите rbit с помощью диспетчера служб."),
        DashboardMessage::BackToDashboard => text.literal("Вернуться на панель управления"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("Запросов за период"),
        MonitoringMessage::ErrorRate => text.literal("Доля ошибок сервера"),
        MonitoringMessage::ProcessUptime => text.literal("Время работы процесса"),
        MonitoringMessage::Samples => text.literal("5-секундные замеры"),
        MonitoringMessage::FiveMinutesAgo => text.literal("5 минут назад"),
        MonitoringMessage::LatestSample => text.literal("Последний замер"),
        MonitoringMessage::Collecting => text.literal("Сбор первого замера. Графики появятся через пять секунд."),
        MonitoringMessage::HttpResponses => text.literal("HTTP-ответы"),
        MonitoringMessage::CurrentWindow => text.literal("Текущий период"),
        MonitoringMessage::Status => text.literal("Статус"),
        MonitoringMessage::RequestsColumn => text.literal("Запросы"),
        MonitoringMessage::Share => text.literal("Доля"),
        MonitoringMessage::GitActivity => text.literal("Активность Git"),
        MonitoringMessage::Operation => text.literal("Операция"),
        MonitoringMessage::Exchanges => text.literal("Обмены"),
        MonitoringMessage::CloneFetch => text.literal("Клонирование / получение"),
        MonitoringMessage::Push => text.literal("Отправка"),
        MonitoringMessage::GitNote => text.literal("Обмены Smart HTTP, включая неудачные. SSH и объявления ссылок не учитываются. Время ответа измеряется до отправки заголовков; потоковые передачи могут продолжаться."),
        MonitoringMessage::Footnote => text.literal("Только этот процесс; история сбрасывается при перезапуске. Запросы мониторинга не учитываются."),
        MonitoringMessage::Informational => text.literal("Информационные"),
        MonitoringMessage::Successful => text.literal("Успешные"),
        MonitoringMessage::Redirects => text.literal("Перенаправления"),
        MonitoringMessage::ClientErrors => text.literal("Ошибки клиента"),
        MonitoringMessage::ServerErrors => text.literal("Ошибки сервера"),
        MonitoringMessage::RequestTraffic => text.literal("Трафик запросов"),
        MonitoringMessage::ResponseTime => text.literal("Время ответа"),
        MonitoringMessage::RequestsPerSecond => text.literal("запр./с"),
        MonitoringMessage::MillisecondsAverage => text.literal("мс в среднем"),
        MonitoringMessage::WaitingFirst => text.literal("Ожидание первого замера"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("Примечания к выпуску rbit ");
            text.argument(version);
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.literal("Администраторов: ");
            text.argument(admins);
            text.literal(" · приостановлено: ");
            text.argument(suspended);
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("Администраторов: 1 · приостановлено: ");
            text.argument(suspended);
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.literal("Публичных: ");
            text.argument(public);
            text.literal(" · приватных: ");
            text.argument(private);
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" из ");
            text.argument(total);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("Отправляются в ");
            text.argument(hour);
            text.literal(", если пользователь не выбрал другое время");
        }
        AdminText::DeleteLabel(name) => {
            text.literal("Удалить ");
            text.argument(name);
        }
        AdminText::ActionsFor(name) => {
            text.literal("Действия для ");
            text.argument(name);
        }
        AdminText::PasswordLength(length) => {
            text.literal("Не менее ");
            text.number(length);
            text.literal(" ");
            text.literal(match text.plural(length) {
                Plural::One => "символа",
                _ => "символов",
            });
            text.literal(".");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.argument(title);
            text.literal(" за последние пять минут. Последнее значение: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". Когда запросов нет, на графике времени ответа остаются пропуски.");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("Обновлено в ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" из ");
            text.number(limit);
            text.literal(" ");
            text.literal(match text.plural(limit) {
                Plural::One => "замера",
                _ => "замеров",
            });
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · Администрирование");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("Пароль для ");
            text.argument(name);
            text.literal(" · Администрирование");
        }
        AdminText::DeletePageTitle(name) => {
            text.literal("Удаление: ");
            text.argument(name);
            text.literal(" · Администрирование");
        }
    }
}
