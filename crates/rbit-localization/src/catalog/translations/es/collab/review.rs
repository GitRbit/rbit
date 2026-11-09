//! Spanish (`es`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("Cambios"),
        ReviewMessage::ToggleFileTree => text.literal("Mostrar u ocultar el árbol de archivos"),
        ReviewMessage::CollapseAll => text.literal("Contraer todo"),
        ReviewMessage::ExpandAll => text.literal("Expandir todo"),
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentario obsoleto está en la conversación",
                _ => " comentarios obsoletos están en la conversación",
            });
        }
        ReviewMessage::FinishReview => text.literal("Termina tu revisión"),
        ReviewMessage::Truncated => text.literal(
            "Este cambio es muy grande, así que no se muestran algunas diferencias. Clona el repositorio para ver el \
             cambio completo.",
        ),
        ReviewMessage::FileToggleHint => {
            text.literal("Haz clic para contraer o expandir | Alt+clic para todos los archivos");
        }
        ReviewMessage::ViewFile => text.literal("Ver archivo"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Comentar la línea ");
            text.number(line);
        }
        ReviewMessage::StartReview => text.literal("Iniciar una revisión"),
        ReviewMessage::AddSingleComment => text.literal("Añadir un comentario suelto"),
        ReviewMessage::BinaryFile => text.literal("No se muestra el archivo binario."),
        ReviewMessage::TooLarge => text.literal("Las diferencias son demasiado grandes para mostrarlas."),
        ReviewMessage::SubmoduleCommit => text.literal("Commit del submódulo"),
        ReviewMessage::NoContentChanges => text.literal("Sin cambios en el contenido."),
        ReviewMessage::NoChangesTitle => text.literal("No hay cambios"),
        ReviewMessage::NoChangesText => text.literal("La rama de origen no añade nada a la base."),
        ReviewMessage::Added => text.literal("Añadido"),
        ReviewMessage::Deleted => text.literal("Eliminado"),
        ReviewMessage::Modified => text.literal("Modificado"),
        ReviewMessage::Renamed => text.literal("Renombrado"),
        ReviewMessage::Copied => text.literal("Copiado"),
        ReviewMessage::Outdated => text.literal("Obsoleto"),
        ReviewMessage::Pending => text.literal("Pendiente"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("Conversación resuelta · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("Responder…"),
        ReviewMessage::AddToReview => text.literal("Añadir a la revisión"),
        ReviewMessage::ReopenConversation => text.literal("Reabrir la conversación"),
        ReviewMessage::ResolveConversation => text.literal("Resolver la conversación"),
        ReviewMessage::ReviewChanges => text.literal("Revisar los cambios"),
        ReviewMessage::PendingCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " pendiente",
                _ => " pendientes",
            });
        }
        ReviewMessage::ReviewPlaceholder => text.literal("Deja un comentario (admite Markdown)"),
        ReviewMessage::RequestChanges => text.literal("Solicitar cambios"),
        ReviewMessage::Approve => text.literal("Aprobar"),
        ReviewMessage::DiscardReview => text.literal("Descartar la revisión pendiente"),
        ReviewMessage::DismissReview => text.literal("Descartar la revisión"),
        ReviewMessage::DismissReason => text.literal("¿Por qué se descarta esta revisión?"),
        ReviewMessage::Dismiss => text.literal("Descartar"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("Descartada por ");
            text.argument(by);
            text.literal(": ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("Revisores"),
        ReviewMessage::NoReviews => text.literal("Aún no hay revisiones"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Retirar la solicitud de revisión a ");
            text.argument(name);
        }
        ReviewMessage::RemoveRequest => text.literal("Retirar la solicitud"),
        ReviewMessage::RequestReviewFrom => text.literal("Solicitar una revisión a"),
        ReviewMessage::Username => text.literal("nombre de usuario"),
        ReviewMessage::RequestReview => text.literal("Solicitar revisión"),
        ReviewMessage::Approved => text.literal("Aprobada"),
        ReviewMessage::ChangesRequested => text.literal("Cambios solicitados"),
        ReviewMessage::Dismissed => text.literal("Descartada"),
        ReviewMessage::Commented => text.literal("Comentada"),
        ReviewMessage::AwaitingReview => text.literal("Pendiente de revisión"),
    }
}
