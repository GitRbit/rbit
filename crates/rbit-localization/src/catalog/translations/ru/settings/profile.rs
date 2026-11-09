//! Russian (`ru`) entries for `catalog/settings/profile.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::GallerySentence;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};
use crate::plural::Plural;

entries! { Ru:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("Профиль"),
        ProfileMessage::IntroLead => text.literal("Показывается на"),
        ProfileMessage::IntroLink => text.literal("странице вашего профиля"),
        ProfileMessage::IntroTail => text.literal("и рядом с вашей активностью. Всё здесь публично и необязательно."),
        ProfileMessage::Updated => text.literal("Профиль обновлён."),
        ProfileMessage::DisplayName => text.literal("Отображаемое имя"),
        ProfileMessage::DisplayNameHint => text.literal("Показывается вместо имени пользователя, где хватает места."),
        ProfileMessage::Bio => text.literal("О себе"),
        ProfileMessage::BioHint => text.literal("До 300 символов."),
        ProfileMessage::Website => text.literal("Веб-сайт"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("Местоположение"),
        ProfileMessage::Save => text.literal("Сохранить профиль"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("Оформление"),
        AppearanceMessage::Intro => text.literal("Тема сохраняется в вашей учётной записи и применяется на всех устройствах, где вы входите. Наведите указатель на тему, чтобы посмотреть её; нажмите, чтобы выбрать."),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("Уведомления"),
        NotificationsMessage::IntroLead => text.literal("Всё по-прежнему появляется"),
        NotificationsMessage::IntroLink => text.literal("во входящих"),
        NotificationsMessage::IntroTail => text.literal(". Выберите, что также отправлять на ваш адрес электронной почты."),
        NotificationsMessage::Saved => text.literal("Настройки сохранены."),
        NotificationsMessage::Unavailable => text.literal("Электронная почта на этом сервере недоступна, поэтому уведомления появляются только во входящих."),
        NotificationsMessage::VerifyLead => text.literal("Подтвердите адрес "),
        NotificationsMessage::VerifyMiddle => text.literal(" в "),
        NotificationsMessage::VerifyLink => text.literal("настройках учётной записи"),
        NotificationsMessage::VerifyTail => text.literal(" — это нужно для получения писем с уведомлениями."),
        NotificationsMessage::Email => text.literal("Электронная почта"),
        NotificationsMessage::SendTo => text.literal("Отправлять на"),
        NotificationsMessage::When => text.literal("Когда"),
        NotificationsMessage::DigestTime => text.literal("Время отправки сводки"),
        NotificationsMessage::DigestHint => text.literal("В сводки не попадают обсуждения, которые вы уже прочитали здесь. В каждом письме есть ссылка, отключающая письма в один клик."),
        NotificationsMessage::Save => text.literal("Сохранить настройки"),
        NotificationsMessage::Off => text.literal("Выключено"),
        NotificationsMessage::OffText => text.literal("Уведомления появляются только во входящих."),
        NotificationsMessage::Participating => text.literal("С моим участием"),
        NotificationsMessage::ParticipatingText => text.literal("Обсуждения, которые вы открыли, прокомментировали или проверили, а также когда вас упоминают, назначают исполнителем или просят о ревью."),
        NotificationsMessage::Everything => text.literal("Все"),
        NotificationsMessage::EverythingText => text.literal("А также обсуждения, на которые вы подписались, не участвуя в них."),
        NotificationsMessage::Immediately => text.literal("Сразу"),
        NotificationsMessage::ImmediatelyText => text.literal("Отдельное письмо для каждого уведомления, как только оно появляется."),
        NotificationsMessage::Daily => text.literal("Ежедневная сводка"),
        NotificationsMessage::DailyText => text.literal("Одно письмо в день со всем, что появилось с прошлого письма."),
        NotificationsMessage::Weekly => text.literal("Еженедельная сводка"),
        NotificationsMessage::WeeklyText => text.literal("Одно письмо каждый понедельник со всем, что появилось за прошлую неделю."),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("По умолчанию (");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("Показать"),
        GalleryMessage::Search => text.literal("Поиск тем"),
        GalleryMessage::BuiltIn => text.literal("Встроенные"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("Нет тем, соответствующих запросу."),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::Licence => text.literal("Лицензия и благодарности"),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("Поиск среди ");
            text.number(total);
            text.literal(" ");
            text.literal(match text.plural(total) {
                Plural::One => "темы",
                _ => "тем",
            });
        }
    }
    GallerySentence => RichWriter |text| {
        GallerySentence::Credit => {
            text.literal("Темы Tinted созданы на основе схем base16 и base24 проекта ");
            text.slot(0);
            text.literal(" (MIT); наведите указатель на тему, чтобы увидеть её автора. ");
            text.slot(1);
            text.literal(".");
        }
    }
}
