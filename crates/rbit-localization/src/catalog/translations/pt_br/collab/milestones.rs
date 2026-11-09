//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("Novo marco"),
        MilestoneMessage::EditMilestone => text.literal("Editar marco"),
        MilestoneMessage::EmptyTitle => text.literal("Nenhum marco aqui"),
        MilestoneMessage::EmptyText => {
            text.literal(
                "Os marcos agrupam issues rumo a uma release ou data e mostram quanto já foi concluído.",
            );
        }
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("Fechado ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("Vence em ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("Venceu em ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("Sem data de vencimento"),
        MilestoneMessage::Close => text.literal("Fechar"),
        MilestoneMessage::Reopen => text.literal("Reabrir"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("Excluir o marco “");
            text.argument(title);
            text.literal("”? As issues dele serão mantidas.");
        }
        MilestoneMessage::SearchIssues => text.literal("Pesquisar nestas issues"),
        MilestoneMessage::NothingHereTitle => text.literal("Nada aqui"),
        MilestoneMessage::NothingHereText => {
            text.literal("As issues e pull requests adicionados a este marco aparecem aqui.");
        }
        MilestoneMessage::FormIntro => text.literal("Agrupe issues rumo a uma release ou data."),
        MilestoneMessage::DueDate => text.literal("Data de vencimento"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, opcional"),
        MilestoneMessage::CreateMilestone => text.literal("Criar marco"),
        MilestoneMessage::SaveMilestone => text.literal("Salvar marco"),
        MilestoneMessage::Complete { percent } => {
            text.number(percent);
            text.literal("% concluído");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aberta",
                _ => " abertas",
            });
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fechada",
                _ => " fechadas",
            });
        }
    }
}
