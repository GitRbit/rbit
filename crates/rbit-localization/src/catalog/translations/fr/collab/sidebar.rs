//! French (`fr`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("Personnes assignées"),
        IssueSidebarMessage::NoOne => text.literal("Personne"),
        IssueSidebarMessage::AssignYourself => text.literal("vous assigner"),
        IssueSidebarMessage::EditAssignees => text.literal("Modifier les personnes assignées"),
        IssueSidebarMessage::SaveAssignees => text.literal("Enregistrer les personnes assignées"),
        IssueSidebarMessage::Labels => text.literal("Étiquettes"),
        IssueSidebarMessage::NoneYet => text.literal("Aucune pour l’instant"),
        IssueSidebarMessage::EditLabels => text.literal("Modifier les étiquettes"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("Ce dépôt n’a pas encore d’étiquettes."),
        IssueSidebarMessage::SaveLabels => text.literal("Enregistrer les étiquettes"),
        IssueSidebarMessage::Milestone => text.literal("Jalon"),
        IssueSidebarMessage::NoMilestone => text.literal("Aucun jalon"),
        IssueSidebarMessage::ChangeMilestone => text.literal("Changer de jalon"),
        IssueSidebarMessage::SaveMilestone => text.literal("Enregistrer le jalon"),
        IssueSidebarMessage::Notifications => text.literal("Notifications"),
        IssueSidebarMessage::Subscribe => text.literal("S’abonner"),
        IssueSidebarMessage::Unsubscribe => text.literal("Se désabonner"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("Vous ne recevez pas de notifications pour ce ticket."),
        IssueSidebarMessage::NotNotifiedPull => text.literal("Vous ne recevez pas de notifications pour cette pull request."),
        IssueSidebarMessage::Danger => text.literal("Danger"),
        IssueSidebarMessage::DeleteIssue => text.literal("Supprimer le ticket"),
        IssueSidebarMessage::DeletePull => text.literal("Supprimer la pull request"),
        IssueSidebarMessage::DeleteIssueConfirm => text.literal(
            "Supprimer définitivement ce ticket\u{202f}? Cette action est irréversible.",
        ),
        IssueSidebarMessage::DeletePullConfirm => text.literal(
            "Supprimer définitivement cette pull request\u{202f}? Cette action est irréversible.",
        ),
        IssueSidebarMessage::ReasonManual => text.literal("Vous vous êtes abonné à cette conversation."),
        IssueSidebarMessage::ReasonComment => text.literal("Vous avez commenté cette conversation."),
        IssueSidebarMessage::ReasonReviewed => text.literal("Vous avez fait une revue de cette pull request."),
        IssueSidebarMessage::ReasonAuthor => text.literal("Vous avez ouvert cette conversation."),
        IssueSidebarMessage::ReasonMention => text.literal("Vous avez été mentionné."),
        IssueSidebarMessage::ReasonAssigned => text.literal("Vous êtes assigné."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("Votre revue a été demandée."),
    }
}
