//! Russian (`ru`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;
use crate::catalog::shell::inbox::ThreadState;

entries! { Ru:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("Уведомления"),
        InboxMessage::Show => text.literal("Показать"),
        InboxMessage::Unread => text.literal("Непрочитанные"),
        InboxMessage::All => text.literal("Все"),
        InboxMessage::MarkAllRead => text.literal("Отметить все как прочитанные"),
        InboxMessage::NoNotifications => text.literal("Уведомлений нет"),
        InboxMessage::NoNotificationsText => text.literal("Здесь появляются уведомления о задачах и запросах на слияние, в которых вы участвуете: созданных вами, прокомментированных вами, назначенных вам или тех, где вас упомянули."),
        InboxMessage::AllCaughtUp => text.literal("Всё прочитано"),
        InboxMessage::AllCaughtUpText => text.literal("Непрочитанного нет. Старые уведомления — на вкладке «Все»."),
        InboxMessage::MarkAsRead => text.literal("Отметить как прочитанное"),
        InboxMessage::Unsubscribe => text.literal("Отписаться"),
        InboxMessage::State(ThreadState::OpenIssue) => text.literal("Открытая задача"),
        InboxMessage::State(ThreadState::ClosedIssue) => text.literal("Закрытая задача"),
        InboxMessage::State(ThreadState::OpenPull) => text.literal("Открытый запрос на слияние"),
        InboxMessage::State(ThreadState::MergedPull) => text.literal("Слитый запрос на слияние"),
        InboxMessage::State(ThreadState::ClosedPull) => text.literal("Закрытый запрос на слияние"),
        InboxMessage::StopEmails => text.literal("Отключение писем с уведомлениями"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("Отключить письма с уведомлениями для ");
            text.argument(username);
            text.literal("? Уведомления по-прежнему будут появляться во входящих.");
        }
        InboxMessage::TurnOff => text.literal("Отключить письма с уведомлениями"),
        InboxMessage::EmailsOff => text.literal("Письма с уведомлениями отключены"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(" больше не будет получать письма с уведомлениями. Их можно снова включить в любой момент.");
        }
        InboxMessage::NotificationSettings => text.literal("Настройки уведомлений"),
        InboxMessage::LinkNotValid => text.literal("Эта ссылка недействительна"),
        InboxMessage::LinkNotValidText => text.literal("Возможно, ссылка неполная. Войдите, чтобы выбрать, какие уведомления приходят вам на почту."),
    }
}
