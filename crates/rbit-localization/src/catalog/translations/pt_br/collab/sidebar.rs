//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("Responsáveis"),
        IssueSidebarMessage::NoOne => text.literal("Ninguém"),
        IssueSidebarMessage::AssignYourself => text.literal("atribuir a si mesmo"),
        IssueSidebarMessage::EditAssignees => text.literal("Editar responsáveis"),
        IssueSidebarMessage::SaveAssignees => text.literal("Salvar responsáveis"),
        IssueSidebarMessage::Labels => text.literal("Etiquetas"),
        IssueSidebarMessage::NoneYet => text.literal("Nenhuma ainda"),
        IssueSidebarMessage::EditLabels => text.literal("Editar etiquetas"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("Este repositório ainda não tem etiquetas."),
        IssueSidebarMessage::SaveLabels => text.literal("Salvar etiquetas"),
        IssueSidebarMessage::Milestone => text.literal("Marco"),
        IssueSidebarMessage::NoMilestone => text.literal("Sem marco"),
        IssueSidebarMessage::ChangeMilestone => text.literal("Alterar marco"),
        IssueSidebarMessage::SaveMilestone => text.literal("Salvar marco"),
        IssueSidebarMessage::Notifications => text.literal("Notificações"),
        IssueSidebarMessage::Subscribe => text.literal("Inscrever-se"),
        IssueSidebarMessage::Unsubscribe => text.literal("Cancelar inscrição"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("Você não recebe notificações sobre esta issue."),
        IssueSidebarMessage::NotNotifiedPull => {
            text.literal("Você não recebe notificações sobre este pull request.");
        }
        IssueSidebarMessage::Danger => text.literal("Perigo"),
        IssueSidebarMessage::DeleteIssue => text.literal("Excluir issue"),
        IssueSidebarMessage::DeletePull => text.literal("Excluir pull request"),
        IssueSidebarMessage::DeleteIssueConfirm => {
            text.literal("Excluir esta issue definitivamente? Isso não pode ser desfeito.");
        }
        IssueSidebarMessage::DeletePullConfirm => {
            text.literal("Excluir este pull request definitivamente? Isso não pode ser desfeito.");
        }
        IssueSidebarMessage::ReasonManual => text.literal("Você se inscreveu nesta conversa."),
        IssueSidebarMessage::ReasonComment => text.literal("Você comentou nesta conversa."),
        IssueSidebarMessage::ReasonReviewed => text.literal("Você revisou este pull request."),
        IssueSidebarMessage::ReasonAuthor => text.literal("Você abriu esta conversa."),
        IssueSidebarMessage::ReasonMention => text.literal("Você foi mencionado."),
        IssueSidebarMessage::ReasonAssigned => text.literal("Você é um dos responsáveis."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("Sua revisão foi solicitada."),
    }
}
