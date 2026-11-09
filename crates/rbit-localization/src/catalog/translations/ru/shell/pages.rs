//! Russian (`ru`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::Failure;
use crate::catalog::shell::pages::PageMessage;
use crate::plural::Plural;

entries! { Ru:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(Failure::NotFound) => text.literal("Страница не найдена"),
        PageMessage::ErrorTitle(Failure::Forbidden) => text.literal("Доступ запрещён"),
        PageMessage::ErrorTitle(Failure::BadRequest) => text.literal("Некорректный запрос"),
        PageMessage::ErrorTitle(Failure::SlowDown) => text.literal("Не так быстро"),
        PageMessage::ErrorTitle(Failure::Unavailable) => text.literal("Временно недоступно"),
        PageMessage::ErrorTitle(Failure::Internal) => text.literal("Что-то пошло не так"),
        PageMessage::NotFoundText => text.literal("Запрошенная страница не существует, или у вас нет к ней доступа."),
        PageMessage::UnavailableText => text.literal("Сервис временно недоступен. Повторите попытку чуть позже."),
        PageMessage::InternalText => text.literal("Произошла непредвиденная ошибка. Она записана в журнал."),
        PageMessage::GoHome => text.literal("На главную"),
        PageMessage::Explore => text.literal("Обзор"),
        PageMessage::Offline => text.literal("Нет подключения к сети"),
        PageMessage::OfflineText => text.literal("Не удалось связаться с сервером. Проверьте подключение и повторите попытку."),
        PageMessage::Retry => text.literal("Повторить"),
        PageMessage::Private => text.literal("Приватный"),
        PageMessage::Archived => text.literal("В архиве"),
        PageMessage::Stars(count) => {
            text.number(count.unsigned_abs());
            text.literal(" ");
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "звезда",
                Plural::Few => "звезды",
                _ => "звёзд",
            });
        }
        PageMessage::Pagination => text.literal("Нумерация страниц"),
        PageMessage::Previous => text.literal("Назад"),
        PageMessage::Next => text.literal("Далее"),
        PageMessage::PageOf(page, pages) => {
            text.literal("Страница ");
            text.number(u64::from(page));
            text.literal(" из ");
            text.number(u64::from(pages));
        }
        PageMessage::EditProfile => text.literal("Редактировать профиль"),
        PageMessage::Joined(date) => {
            text.literal("Регистрация: ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("Профиль"),
        PageMessage::Repositories => text.literal("Репозитории"),
        PageMessage::StarsTab => text.literal("Звёзды"),
        PageMessage::NewRepository => text.literal("Новый репозиторий"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(": репозиториев со звёздами пока нет.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(": публичных репозиториев пока нет.");
        }
    }
}
