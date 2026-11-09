//! Russian (`ru`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::TokensSentence;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};
use crate::plural::Plural;

entries! { Ru:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("Токены доступа"),
        TokensMessage::CopyNow => text.literal("Скопируйте новый токен сейчас. Больше вы его не увидите."),
        TokensMessage::CopyToken => text.literal("Скопировать токен"),
        TokensMessage::Copy => text.literal("Скопировать"),
        TokensMessage::NewToken => text.literal("Новый токен"),
        TokensMessage::Name => text.literal("Название"),
        TokensMessage::NamePlaceholder => text.literal("например, Рабочий ноутбук"),
        TokensMessage::NameHint => text.literal("Что-нибудь, по чему его можно будет узнать."),
        TokensMessage::Scope => text.literal("Область действия"),
        TokensMessage::ScopeHint => text.literal("Токен никогда не может больше, чем ваша учётная запись."),
        TokensMessage::Expiration => text.literal("Срок действия"),
        TokensMessage::Generate => text.literal("Создать токен"),
        TokensMessage::YourTokens => text.literal("Ваши токены"),
        TokensMessage::EmptyTitle => text.literal("Токенов доступа нет"),
        TokensMessage::EmptyText => text.literal("Создайте токен, чтобы клонировать, получать и отправлять изменения по HTTPS без пароля."),
        TokensMessage::Revoke => text.literal("Отозвать"),
        TokensMessage::NoExpiration => text.literal("Бессрочно"),
        TokensMessage::OneDay => text.literal("1 день"),
        TokensMessage::NeverUsed => text.literal("Не использовался"),
        TokensMessage::ReadDescription => text.literal("Клонирование и получение изменений; чтение через API."),
        TokensMessage::WriteDescription => text.literal("А также отправка изменений, создание репозиториев и звёзды через API."),
        TokensMessage::AdminDescription => text.literal("А также изменение настроек и вебхуков репозиториев и удаление репозиториев через API."),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("Токен «");
            text.argument(name);
            text.literal("» создан");
        }
        TokenText::RevokeLabel(name) => {
            text.literal("Отозвать ");
            text.argument(name);
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(" ");
            text.literal(match text.plural(days) {
                Plural::One => "день",
                Plural::Few => "дня",
                _ => "дней",
            });
        }
        TokenText::CreatedOn(date) => {
            text.literal("Создан ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("Последнее использование: ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("Истекает ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("Истёк ");
            text.argument(date);
        }
    }
    TokensSentence => RichWriter |text| {
        TokensSentence::Intro => {
            text.literal("Используйте токен вместо пароля для Git по HTTPS: введите имя пользователя ");
            text.slot(0);
            text.literal(" и вставьте токен, когда Git запросит пароль. REST API по адресу ");
            text.slot(1);
            text.literal(" принимает его в заголовке ");
            text.slot(2);
            text.literal(". Токены начинаются с ");
            text.slot(3);
            text.literal("; токены из ранних выпусков начинаются с ");
            text.slot(4);
            text.literal(" и продолжают работать.");
        }
    }
}
