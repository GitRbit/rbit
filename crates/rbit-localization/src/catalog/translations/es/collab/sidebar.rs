//! Spanish (`es`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("Asignados"),
        IssueSidebarMessage::NoOne => text.literal("Nadie"),
        IssueSidebarMessage::AssignYourself => text.literal("asígnatela"),
        IssueSidebarMessage::EditAssignees => text.literal("Editar asignados"),
        IssueSidebarMessage::SaveAssignees => text.literal("Guardar asignados"),
        IssueSidebarMessage::Labels => text.literal("Etiquetas"),
        IssueSidebarMessage::NoneYet => text.literal("Ninguna todavía"),
        IssueSidebarMessage::EditLabels => text.literal("Editar etiquetas"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("Este repositorio aún no tiene etiquetas."),
        IssueSidebarMessage::SaveLabels => text.literal("Guardar etiquetas"),
        IssueSidebarMessage::Milestone => text.literal("Hito"),
        IssueSidebarMessage::NoMilestone => text.literal("Sin hito"),
        IssueSidebarMessage::ChangeMilestone => text.literal("Cambiar hito"),
        IssueSidebarMessage::SaveMilestone => text.literal("Guardar hito"),
        IssueSidebarMessage::Notifications => text.literal("Notificaciones"),
        IssueSidebarMessage::Subscribe => text.literal("Suscribirse"),
        IssueSidebarMessage::Unsubscribe => text.literal("Cancelar suscripción"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("No recibes notificaciones de esta incidencia."),
        IssueSidebarMessage::NotNotifiedPull => {
            text.literal("No recibes notificaciones de esta solicitud de cambios.");
        }
        IssueSidebarMessage::Danger => text.literal("Peligro"),
        IssueSidebarMessage::DeleteIssue => text.literal("Eliminar incidencia"),
        IssueSidebarMessage::DeletePull => text.literal("Eliminar solicitud de cambios"),
        IssueSidebarMessage::DeleteIssueConfirm => {
            text.literal("¿Eliminar esta incidencia para siempre? Esto no se puede deshacer.");
        }
        IssueSidebarMessage::DeletePullConfirm => {
            text.literal("¿Eliminar esta solicitud de cambios para siempre? Esto no se puede deshacer.");
        }
        IssueSidebarMessage::ReasonManual => text.literal("Te suscribiste a esta conversación."),
        IssueSidebarMessage::ReasonComment => text.literal("Comentaste en esta conversación."),
        IssueSidebarMessage::ReasonReviewed => text.literal("Revisaste esta solicitud de cambios."),
        IssueSidebarMessage::ReasonAuthor => text.literal("Abriste esta conversación."),
        IssueSidebarMessage::ReasonMention => text.literal("Te mencionaron."),
        IssueSidebarMessage::ReasonAssigned => text.literal("La tienes asignada."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("Se solicitó tu revisión."),
    }
}
