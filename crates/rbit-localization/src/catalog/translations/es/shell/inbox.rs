//! Spanish (`es`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { Es:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("Notificaciones"),
        InboxMessage::Show => text.literal("Mostrar"),
        InboxMessage::Unread => text.literal("Sin leer"),
        InboxMessage::All => text.literal("Todas"),
        InboxMessage::MarkAllRead => text.literal("Marcar todas como leídas"),
        InboxMessage::NoNotifications => text.literal("No hay notificaciones"),
        InboxMessage::NoNotificationsText => text.literal(
            "Aquí te enteras de las incidencias y solicitudes de cambios en las que participas: las que abriste, \
             comentaste, tienes asignadas o en las que te mencionaron.",
        ),
        InboxMessage::AllCaughtUp => text.literal("Estás al día"),
        InboxMessage::AllCaughtUpText => {
            text.literal("No hay nada sin leer. Las notificaciones anteriores están en Todas.");
        }
        InboxMessage::MarkAsRead => text.literal("Marcar como leída"),
        InboxMessage::Unsubscribe => text.literal("Cancelar suscripción"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "Incidencia abierta",
            ThreadState::ClosedIssue => "Incidencia cerrada",
            ThreadState::OpenPull => "Solicitud de cambios abierta",
            ThreadState::MergedPull => "Solicitud de cambios fusionada",
            ThreadState::ClosedPull => "Solicitud de cambios cerrada",
        }),
        InboxMessage::StopEmails => text.literal("Dejar de recibir correos de notificación"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("¿Desactivar los correos de notificación de ");
            text.argument(username);
            text.literal("? Las notificaciones seguirán apareciendo en la bandeja de entrada.");
        }
        InboxMessage::TurnOff => text.literal("Desactivar los correos de notificación"),
        InboxMessage::EmailsOff => text.literal("Los correos de notificación están desactivados"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(" ya no recibirá correos de notificación. Puedes volver a activarlos cuando quieras.");
        }
        InboxMessage::NotificationSettings => text.literal("Configuración de notificaciones"),
        InboxMessage::LinkNotValid => text.literal("Este enlace no es válido"),
        InboxMessage::LinkNotValidText => text.literal(
            "Puede que el enlace esté incompleto. Inicia sesión para elegir qué notificaciones recibes por correo.",
        ),
    }
}
