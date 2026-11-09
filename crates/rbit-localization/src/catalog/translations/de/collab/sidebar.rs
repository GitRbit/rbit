//! German (`de`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("Zugewiesene"),
        IssueSidebarMessage::NoOne => text.literal("Niemand"),
        IssueSidebarMessage::AssignYourself => text.literal("sich selbst zuweisen"),
        IssueSidebarMessage::EditAssignees => text.literal("Zugewiesene bearbeiten"),
        IssueSidebarMessage::SaveAssignees => text.literal("Zugewiesene speichern"),
        IssueSidebarMessage::Labels => text.literal("Labels"),
        IssueSidebarMessage::NoneYet => text.literal("Noch keine"),
        IssueSidebarMessage::EditLabels => text.literal("Labels bearbeiten"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("Dieses Repository hat noch keine Labels."),
        IssueSidebarMessage::SaveLabels => text.literal("Labels speichern"),
        IssueSidebarMessage::Milestone => text.literal("Meilenstein"),
        IssueSidebarMessage::NoMilestone => text.literal("Kein Meilenstein"),
        IssueSidebarMessage::ChangeMilestone => text.literal("Meilenstein ändern"),
        IssueSidebarMessage::SaveMilestone => text.literal("Meilenstein speichern"),
        IssueSidebarMessage::Notifications => text.literal("Benachrichtigungen"),
        IssueSidebarMessage::Subscribe => text.literal("Abonnieren"),
        IssueSidebarMessage::Unsubscribe => text.literal("Abbestellen"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("Sie werden zu diesem Issue nicht benachrichtigt."),
        IssueSidebarMessage::NotNotifiedPull => {
            text.literal("Sie werden zu diesem Pull-Request nicht benachrichtigt.");
        }
        IssueSidebarMessage::Danger => text.literal("Gefahrenbereich"),
        IssueSidebarMessage::DeleteIssue => text.literal("Issue löschen"),
        IssueSidebarMessage::DeletePull => text.literal("Pull-Request löschen"),
        IssueSidebarMessage::DeleteIssueConfirm => {
            text.literal("Dieses Issue endgültig löschen? Das kann nicht rückgängig gemacht werden.");
        }
        IssueSidebarMessage::DeletePullConfirm => {
            text.literal("Diesen Pull-Request endgültig löschen? Das kann nicht rückgängig gemacht werden.");
        }
        IssueSidebarMessage::ReasonManual => text.literal("Sie haben diesen Thread abonniert."),
        IssueSidebarMessage::ReasonComment => text.literal("Sie haben diesen Thread kommentiert."),
        IssueSidebarMessage::ReasonReviewed => text.literal("Sie haben diesen Pull-Request geprüft."),
        IssueSidebarMessage::ReasonAuthor => text.literal("Sie haben diesen Thread eröffnet."),
        IssueSidebarMessage::ReasonMention => text.literal("Sie wurden erwähnt."),
        IssueSidebarMessage::ReasonAssigned => text.literal("Sie sind zugewiesen."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("Ein Review von Ihnen wurde angefordert."),
    }
}
