//! Russian (`ru`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};
use crate::plural::Plural;

entries! { Ru:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("Все, у кого есть учётная запись на этом сервере."),
        AdminPageMessage::UsersSearch => text.literal("Поиск по имени пользователя, имени или адресу электронной почты"),
        AdminPageMessage::NewUser => text.literal("Новый пользователь"),
        AdminPageMessage::NoUsers => text.literal("Пользователи не найдены"),
        AdminPageMessage::NoUsersText => text.literal("Попробуйте другой запрос."),
        AdminPageMessage::User => text.literal("Пользователь"),
        AdminPageMessage::Joined => text.literal("Регистрация"),
        AdminPageMessage::Admin => text.literal("Администратор"),
        AdminPageMessage::Suspended => text.literal("Приостановлен"),
        AdminPageMessage::EmailVerified => text.literal("Адрес электронной почты подтверждён"),
        AdminPageMessage::Verified => text.literal("Подтверждён"),
        AdminPageMessage::You => text.literal("Вы"),
        AdminPageMessage::RemoveAdmin => text.literal("Снять права администратора"),
        AdminPageMessage::MakeAdmin => text.literal("Сделать администратором"),
        AdminPageMessage::LiftSuspension => text.literal("Снять приостановку"),
        AdminPageMessage::Suspend => text.literal("Приостановить"),
        AdminPageMessage::SendReset => text.literal("Отправить ссылку для сброса пароля"),
        AdminPageMessage::SetPasswordMenu => text.literal("Задать пароль…"),
        AdminPageMessage::DeleteMenu => text.literal("Удалить…"),
        AdminPageMessage::ResetSent => text.literal("Письмо для сброса пароля отправлено."),
        AdminPageMessage::PasswordSet => text.literal("Пароль обновлён."),
        AdminPageMessage::NewUserIntro => text.literal("Администраторы могут создавать учётные записи, даже когда регистрация закрыта."),
        AdminPageMessage::Username => text.literal("Имя пользователя"),
        AdminPageMessage::UsernameHint => text.literal("Буквы, цифры, «-», «_» и «.»"),
        AdminPageMessage::Email => text.literal("Электронная почта"),
        AdminPageMessage::InitialPassword => text.literal("Начальный пароль"),
        AdminPageMessage::SiteAdministrator => text.literal("Администратор сайта"),
        AdminPageMessage::SiteAdministratorText => text.literal("Полный доступ ко всем репозиториям и к этой панели."),
        AdminPageMessage::CreateUser => text.literal("Создать пользователя"),
        AdminPageMessage::RegistrationClosed => text.literal("Открытая регистрация сейчас закрыта; это единственный способ добавить пользователей."),
        AdminPageMessage::SetPassword => text.literal("Задать пароль"),
        AdminPageMessage::ResetHint => text.literal("Чтобы пользователь сам выбрал пароль, отправьте ему ссылку для сброса из списка пользователей."),
        AdminPageMessage::DeleteUser => text.literal("Удалить пользователя"),
        AdminPageMessage::DeleteUserLead => text.literal("Это действие навсегда удалит пользователя"),
        AdminPageMessage::OrganizationsIntro => text.literal("Общие пространства имён и их участники."),
        AdminPageMessage::SearchByName => text.literal("Поиск по имени"),
        AdminPageMessage::NoOrganizations => text.literal("Организации не найдены"),
        AdminPageMessage::NoOrganizationsText => text.literal("Организации появятся здесь, когда будут созданы."),
        AdminPageMessage::Organization => text.literal("Организация"),
        AdminPageMessage::Members => text.literal("Участники"),
        AdminPageMessage::Created => text.literal("Создана"),
        AdminPageMessage::RepositoriesIntro => text.literal("Все репозитории, публичные и приватные; сначала недавно активные."),
        AdminPageMessage::RepositoriesSearch => text.literal("Поиск по владелец/имя или описанию"),
        AdminPageMessage::NoRepositories => text.literal("Репозитории не найдены"),
        AdminPageMessage::NoRepositoriesText => text.literal("Репозитории появятся здесь, когда будут созданы."),
        AdminPageMessage::Repository => text.literal("Репозиторий"),
        AdminPageMessage::Updated => text.literal("Обновлён"),
        AdminPageMessage::DeleteRepository => text.literal("Удалить репозиторий"),
        AdminPageMessage::DeleteRepositoryText => text.literal("Репозиторий будет удалён навсегда вместе со всеми данными Git. Это действие нельзя отменить."),
        AdminPageMessage::AuditIntro => text.literal("Входы, изменения двухфакторной аутентификации, сеансы, токены доступа и действия администраторов; сначала новые."),
        AdminPageMessage::AuditSearch => text.literal("Пользователь, объект или IP-адрес"),
        AdminPageMessage::Category => text.literal("Категория"),
        AdminPageMessage::Filter => text.literal("Фильтр"),
        AdminPageMessage::AllEvents => text.literal("Все события"),
        AdminPageMessage::Clear => text.literal("Сбросить"),
        AdminPageMessage::NoEvents => text.literal("Событий нет"),
        AdminPageMessage::NoEventsText => text.literal("Под эти фильтры пока ничего не подходит."),
        AdminPageMessage::As => text.literal("под именем"),
        AdminPageMessage::By => text.literal("пользователь"),
        AdminPageMessage::On => text.literal("объект:"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("Изменения сразу применяются ко всему серверу."),
        InstanceSettingsMessage::Saved => text.literal("Настройки сохранены."),
        InstanceSettingsMessage::SiteName => text.literal("Название сайта"),
        InstanceSettingsMessage::SiteNameHint => text.literal("Показывается в шапке, заголовках страниц и при установке в виде приложения."),
        InstanceSettingsMessage::Branding => text.literal("Оформление и идентичность"),
        InstanceSettingsMessage::BrandingHint => text.literal("Текст — обычный, без разметки. Для изображений и ссылок можно указать HTTPS-адреса или локальные /пути, которые обслуживает ваш обратный прокси. Пустые поля изображений используют встроенные значения."),
        InstanceSettingsMessage::PrimaryColor => text.literal("Цвет основной кнопки (пусто — из темы)"),
        InstanceSettingsMessage::PrimaryTextColor => text.literal("Цвет текста основной кнопки"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => text.literal("Выбирайте цвета с достаточным контрастом, чтобы подписи кнопок легко читались. Задайте оба цвета или оставьте оба пустыми."),
        InstanceSettingsMessage::Description => text.literal("Описание сайта"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("Заголовок приветствия (пусто — название сайта)"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("Приветственное сообщение"),
        InstanceSettingsMessage::LogoUrl => text.literal("URL логотипа"),
        InstanceSettingsMessage::FaviconUrl => text.literal("URL значка сайта"),
        InstanceSettingsMessage::AppIconUrl => text.literal("URL значка приложения (квадратный, не менее 512 пикселей)"),
        InstanceSettingsMessage::FooterText => text.literal("Текст подвала"),
        InstanceSettingsMessage::SupportUrl => text.literal("URL поддержки"),
        InstanceSettingsMessage::TermsUrl => text.literal("URL условий использования"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("URL политики конфиденциальности"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("Показывать «Powered by rbit»"),
        InstanceSettingsMessage::ShowVersion => text.literal("Показывать версию ПО"),
        InstanceSettingsMessage::ShowFeatures => text.literal("Показывать возможности самостоятельного размещения на приветственной странице"),
        InstanceSettingsMessage::Explore => text.literal("Страница «Обзор»"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("Включена"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("Все, кто может просматривать сайт, могут искать и просматривать список всех публичных репозиториев."),
        InstanceSettingsMessage::ExploreDisabled => text.literal("Выключена"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("Страница «Обзор» недоступна, а ссылки на неё скрыты, в том числе для администраторов. Публичные репозитории остаются публичными."),
        InstanceSettingsMessage::Browsing => text.literal("Кто может просматривать сайт"),
        InstanceSettingsMessage::BrowsingOpen => text.literal("Все"),
        InstanceSettingsMessage::BrowsingOpenText => text.literal("Посетители могут читать публичные репозитории, профили и организации без учётной записи."),
        InstanceSettingsMessage::BrowsingSignedIn => text.literal("Только вошедшие пользователи"),
        InstanceSettingsMessage::BrowsingSignedInText => text.literal("Посетители перенаправляются на страницу входа. Git, LFS и API отклоняют чтение без учётных данных."),
        InstanceSettingsMessage::Registration => text.literal("Регистрация"),
        InstanceSettingsMessage::Open => text.literal("Открыта"),
        InstanceSettingsMessage::OpenText => text.literal("Любой может создать учётную запись на странице регистрации."),
        InstanceSettingsMessage::Closed => text.literal("Закрыта"),
        InstanceSettingsMessage::ClosedText => text.literal("Создавать учётные записи могут только администраторы."),
        InstanceSettingsMessage::OrgCreation => text.literal("Кто может создавать организации"),
        InstanceSettingsMessage::AnyUser => text.literal("Любой пользователь"),
        InstanceSettingsMessage::AnyUserText => text.literal("Создавать организации может любой вошедший пользователь."),
        InstanceSettingsMessage::AdminsOnly => text.literal("Только администраторы"),
        InstanceSettingsMessage::AdminsOnlyText => text.literal("Пользователей по-прежнему можно добавлять в существующие организации."),
        InstanceSettingsMessage::DefaultVisibility => text.literal("Видимость репозиториев по умолчанию"),
        InstanceSettingsMessage::Public => text.literal("Публичные"),
        InstanceSettingsMessage::PublicText => text.literal("Новые репозитории видны всем, если не изменить это."),
        InstanceSettingsMessage::Private => text.literal("Приватные"),
        InstanceSettingsMessage::PrivateText => text.literal("Новые репозитории создаются приватными; владельцы могут сделать их публичными."),
        InstanceSettingsMessage::VisibilityHint => text.literal("Выбирается заранее в форме нового репозитория."),
        InstanceSettingsMessage::DigestTime => text.literal("Время отправки сводок"),
        InstanceSettingsMessage::DigestHint => text.literal("Когда отправляются ежедневные и еженедельные сводки уведомлений пользователям, которые не выбрали время. Еженедельные сводки отправляются по понедельникам."),
        InstanceSettingsMessage::Save => text.literal("Сохранить настройки"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("Не менее ");
            text.number(length);
            text.literal(" ");
            text.literal(match text.plural(length) {
                Plural::One => "символа",
                _ => "символов",
            });
            text.literal(". Передайте его пользователю безопасным способом. При первом входе пользователь должен будет его заменить.");
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal("Новый пароль сразу заменяет текущий, а все ссылки для сброса пароля перестают работать. Передайте его пользователю ");
            text.argument(name);
            text.literal(" безопасным способом.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", его репозитории (");
            text.argument(repositories);
            text.literal(", включая все данные Git), токены доступа и членство в организациях. Это действие нельзя отменить.");
        }
    }
}
