//! French (`fr`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { Fr:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("Notifications"),
        InboxMessage::Show => text.literal("Afficher"),
        InboxMessage::Unread => text.literal("Non lues"),
        InboxMessage::All => text.literal("Toutes"),
        InboxMessage::MarkAllRead => text.literal("Tout marquer comme lu"),
        InboxMessage::NoNotifications => text.literal("Aucune notification"),
        InboxMessage::NoNotificationsText => text.literal(
            "Vous êtes informé ici des tickets et des pull requests auxquels vous participez\u{a0}: ceux que vous avez \
             ouverts, commentés, qui vous sont assignés ou dans lesquels vous avez été mentionné.",
        ),
        InboxMessage::AllCaughtUp => text.literal("Vous êtes à jour"),
        InboxMessage::AllCaughtUpText => text.literal("Rien de non lu. Les notifications plus anciennes se trouvent sous «\u{a0}Toutes\u{a0}»."),
        InboxMessage::MarkAsRead => text.literal("Marquer comme lu"),
        InboxMessage::Unsubscribe => text.literal("Se désabonner"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "Ticket ouvert",
            ThreadState::ClosedIssue => "Ticket fermé",
            ThreadState::OpenPull => "Pull request ouverte",
            ThreadState::MergedPull => "Pull request fusionnée",
            ThreadState::ClosedPull => "Pull request fermée",
        }),
        InboxMessage::StopEmails => text.literal("Arrêter les e-mails de notification"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("Désactiver les e-mails de notification pour ");
            text.argument(username);
            text.literal("\u{202f}? Les notifications continueront d’apparaître dans la boîte de réception.");
        }
        InboxMessage::TurnOff => text.literal("Désactiver les e-mails de notification"),
        InboxMessage::EmailsOff => text.literal("Les e-mails de notification sont désactivés"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(" ne recevra plus d’e-mails de notification. Vous pouvez les réactiver à tout moment.");
        }
        InboxMessage::NotificationSettings => text.literal("Paramètres de notification"),
        InboxMessage::LinkNotValid => text.literal("Ce lien n’est pas valide"),
        InboxMessage::LinkNotValidText => text.literal(
            "Le lien est peut-être incomplet. Connectez-vous pour choisir les notifications qui vous sont envoyées par e-mail.",
        ),
    }
}
