//! Brazilian Portuguese (`pt-BR`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { PtBr:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("Notificações"),
        InboxMessage::Show => text.literal("Mostrar"),
        InboxMessage::Unread => text.literal("Não lidas"),
        InboxMessage::All => text.literal("Todas"),
        InboxMessage::MarkAllRead => text.literal("Marcar todas como lidas"),
        InboxMessage::NoNotifications => text.literal("Nenhuma notificação"),
        InboxMessage::NoNotificationsText => {
            text.literal(
                "Aqui você fica sabendo das issues e pull requests de que participa: os que você abriu, comentou, \
                 recebeu como responsável ou em que foi mencionado.",
            );
        }
        InboxMessage::AllCaughtUp => text.literal("Tudo em dia"),
        InboxMessage::AllCaughtUpText => {
            text.literal("Nada não lido. As notificações mais antigas estão em Todas.");
        }
        InboxMessage::MarkAsRead => text.literal("Marcar como lida"),
        InboxMessage::Unsubscribe => text.literal("Cancelar inscrição"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "Issue aberta",
            ThreadState::ClosedIssue => "Issue fechada",
            ThreadState::OpenPull => "Pull request aberto",
            ThreadState::MergedPull => "Pull request mesclado",
            ThreadState::ClosedPull => "Pull request fechado",
        }),
        InboxMessage::StopEmails => text.literal("Parar e-mails de notificação"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("Desativar os e-mails de notificação de ");
            text.argument(username);
            text.literal("? As notificações continuarão aparecendo na caixa de entrada.");
        }
        InboxMessage::TurnOff => text.literal("Desativar e-mails de notificação"),
        InboxMessage::EmailsOff => text.literal("Os e-mails de notificação estão desativados"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(
                " não receberá mais e-mails de notificação. Você pode reativá-los a qualquer momento.",
            );
        }
        InboxMessage::NotificationSettings => text.literal("Configurações de notificação"),
        InboxMessage::LinkNotValid => text.literal("Este link não é válido"),
        InboxMessage::LinkNotValidText => {
            text.literal("O link pode estar incompleto. Entre para escolher quais notificações você recebe por e-mail.");
        }
    }
}
