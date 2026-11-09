//! Russian (`ru`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::ChromeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("Перейти к содержимому"),
        ChromeMessage::Main => text.literal("Главное меню"),
        ChromeMessage::Explore => text.literal("Обзор"),
        ChromeMessage::SwitchTheme => text.literal("Переключить светлую и тёмную тему"),
        ChromeMessage::ChooseTheme => text.literal("Выбрать тему"),
        ChromeMessage::Notifications => text.literal("Уведомления"),
        ChromeMessage::Unread => text.literal("У вас есть непрочитанные уведомления"),
        ChromeMessage::NewRepository => text.literal("Новый репозиторий"),
        ChromeMessage::AccountMenu => text.literal("Меню учётной записи"),
        ChromeMessage::Issues => text.literal("Задачи"),
        ChromeMessage::Pulls => text.literal("Запросы на слияние"),
        ChromeMessage::Dashboard => text.literal("Панель управления"),
        ChromeMessage::Profile => text.literal("Профиль"),
        ChromeMessage::NewOrganization => text.literal("Новая организация"),
        ChromeMessage::Settings => text.literal("Настройки"),
        ChromeMessage::Administration => text.literal("Администрирование сайта"),
        ChromeMessage::SignOut => text.literal("Выйти"),
        ChromeMessage::SignIn => text.literal("Войти"),
        ChromeMessage::SignUp => text.literal("Зарегистрироваться"),
        ChromeMessage::Menu => text.literal("Меню"),
        ChromeMessage::CreateAccount => text.literal("Создать учётную запись"),
        ChromeMessage::Themes => text.literal("Темы"),
        ChromeMessage::LightMode => text.literal("Светлая тема"),
        ChromeMessage::DarkMode => text.literal("Тёмная тема"),
        ChromeMessage::PoweredBy => text.literal("Работает на"),
        ChromeMessage::UpdateAvailable => text.literal("Доступно обновление"),
        ChromeMessage::TimingHelp => text.literal("Время обработки последней страницы или обновления на сервере"),
        ChromeMessage::RenderedIn => text.literal("сформировано за"),
        ChromeMessage::Footer => text.literal("Подвал"),
        ChromeMessage::Support => text.literal("Поддержка"),
        ChromeMessage::Terms => text.literal("Условия использования"),
        ChromeMessage::Privacy => text.literal("Конфиденциальность"),
        ChromeMessage::Status => text.literal("Состояние"),
        ChromeMessage::Home { site } => {
            text.literal("Главная: ");
            text.argument(site);
        }
    }
}
