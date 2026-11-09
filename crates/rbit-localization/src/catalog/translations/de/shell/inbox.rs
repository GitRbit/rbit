//! German (`de`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { De:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("Benachrichtigungen"),
        InboxMessage::Show => text.literal("Anzeigen"),
        InboxMessage::Unread => text.literal("Ungelesen"),
        InboxMessage::All => text.literal("Alle"),
        InboxMessage::MarkAllRead => text.literal("Alle als gelesen markieren"),
        InboxMessage::NoNotifications => text.literal("Keine Benachrichtigungen"),
        InboxMessage::NoNotificationsText => text.literal(
            "Hier erfahren Sie von Issues und Pull-Requests, an denen Sie beteiligt sind: die Sie eröffnet oder \
             kommentiert haben, die Ihnen zugewiesen sind oder in denen Sie erwähnt wurden.",
        ),
        InboxMessage::AllCaughtUp => text.literal("Alles erledigt"),
        InboxMessage::AllCaughtUpText => text.literal(
            "Nichts Ungelesenes. Ältere Benachrichtigungen finden Sie unter „Alle“.",
        ),
        InboxMessage::MarkAsRead => text.literal("Als gelesen markieren"),
        InboxMessage::Unsubscribe => text.literal("Abbestellen"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "Offenes Issue",
            ThreadState::ClosedIssue => "Geschlossenes Issue",
            ThreadState::OpenPull => "Offener Pull-Request",
            ThreadState::MergedPull => "Zusammengeführter Pull-Request",
            ThreadState::ClosedPull => "Geschlossener Pull-Request",
        }),
        InboxMessage::StopEmails => text.literal("Benachrichtigungs-E-Mails beenden"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("Benachrichtigungs-E-Mails für ");
            text.argument(username);
            text.literal(" abschalten? Benachrichtigungen erscheinen weiterhin im Posteingang.");
        }
        InboxMessage::TurnOff => text.literal("Benachrichtigungs-E-Mails abschalten"),
        InboxMessage::EmailsOff => text.literal("Benachrichtigungs-E-Mails sind abgeschaltet"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(
                " erhält keine Benachrichtigungs-E-Mails mehr. Sie können sie jederzeit wieder einschalten.",
            );
        }
        InboxMessage::NotificationSettings => text.literal("Benachrichtigungseinstellungen"),
        InboxMessage::LinkNotValid => text.literal("Dieser Link ist ungültig"),
        InboxMessage::LinkNotValidText => text.literal(
            "Der Link ist möglicherweise unvollständig. Melden Sie sich an, um festzulegen, welche \
             Benachrichtigungen Sie per E-Mail erhalten.",
        ),
    }
}
