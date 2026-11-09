//! Spanish (`es`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("Abierta"),
        ThreadMessage::Closed => text.literal("Cerrada"),
        ThreadMessage::Merged => text.literal("Fusionada"),
        ThreadMessage::Draft => text.literal("Borrador"),
        ThreadMessage::Title => text.literal("Título"),
        ThreadMessage::Description => text.literal("Descripción"),
        ThreadMessage::Edit => text.literal("Editar"),
        ThreadMessage::Save => text.literal("Guardar"),
        ThreadMessage::Cancel => text.literal("Cancelar"),
        ThreadMessage::Delete => text.literal("Eliminar"),
        ThreadMessage::Optional => text.literal("opcional"),
        ThreadMessage::MarkdownSupported => text.literal("Se admite Markdown"),
        ThreadMessage::SignIn => text.literal("Inicia sesión"),
        ThreadMessage::LoadMore => text.literal("Cargar más"),
        ThreadMessage::AddComment => text.literal("Añadir un comentario"),
        ThreadMessage::CommentPlaceholder => text.literal(
            "Escribe un comentario. Usa @ para usuarios y # para incidencias o solicitudes de cambios.",
        ),
        ThreadMessage::MentionSuggestions => text.literal("Sugerencias de menciones"),
        ThreadMessage::CommentHint => text.literal(
            "Se admite Markdown. Usa @nombre, #123 o propietario/repositorio#123 para enlazar un usuario, una \
             incidencia o una solicitud de cambios.",
        ),
        ThreadMessage::LockedHint => {
            text.literal("Esta conversación está bloqueada; solo los colaboradores pueden comentar.");
        }
        ThreadMessage::CloseIssue => text.literal("Cerrar incidencia"),
        ThreadMessage::ClosePull => text.literal("Cerrar solicitud de cambios"),
        ThreadMessage::CloseNotPlanned => text.literal("Cerrar como no planificada"),
        ThreadMessage::ReopenIssue => text.literal("Reabrir incidencia"),
        ThreadMessage::ReopenPull => text.literal("Reabrir solicitud de cambios"),
        ThreadMessage::Comment => text.literal("Comentar"),
        ThreadMessage::LockConversation => text.literal("Bloquear la conversación"),
        ThreadMessage::UnlockConversation => text.literal("Desbloquear la conversación"),
        ThreadMessage::Bot => text.literal("bot"),
        ThreadMessage::Edited => text.literal("editado"),
        ThreadMessage::Author => text.literal("Autor"),
        ThreadMessage::Reply => text.literal("Responder"),
        ThreadMessage::DeleteCommentConfirm => text.literal("¿Eliminar este comentario?"),
        ThreadMessage::DeleteComment => text.literal("Eliminar comentario"),
        ThreadMessage::NoDescription => text.literal("No se proporcionó ninguna descripción."),
    }
}
