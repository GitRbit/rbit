//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Issues"),
        IssueMessage::Labels => text.literal("Etiquetas"),
        IssueMessage::Milestones => text.literal("Marcos"),
        IssueMessage::NewIssue => text.literal("Nova issue"),
        IssueMessage::SearchIssues => text.literal("Pesquisar issues"),
        IssueMessage::SearchPlaceholder => text.literal("Pesquisar títulos e descrições…"),
        IssueMessage::Search => text.literal("Pesquisar"),
        IssueMessage::SearchHelp => text.literal("Ajuda da pesquisa"),
        IssueMessage::ClearSearch => text.literal("Limpar a pesquisa, os filtros e a ordenação"),
        IssueMessage::StateNav => text.literal("Estado"),
        IssueMessage::NoIssuesTitle => text.literal("Nenhuma issue corresponde"),
        IssueMessage::NoIssuesText => text.literal("Tente outra pesquisa ou abra a primeira issue."),
        IssueMessage::DescriptionHint => text.literal("Markdown; #12 e @nome viram links"),
        IssueMessage::NoLabelsYet => text.literal("Ainda não há etiquetas."),
        IssueMessage::Assignees => text.literal("Responsáveis"),
        IssueMessage::Milestone => text.literal("Marco"),
        IssueMessage::NoMilestone => text.literal("Sem marco"),
        IssueMessage::OpenIssue => text.literal("Abrir issue"),
        IssueMessage::Details => text.literal("Detalhes"),
        IssueMessage::Locked => text.literal("bloqueada"),
        IssueMessage::AssignedTo { names } => {
            text.literal("Atribuída a ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentário",
                _ => " comentários",
            });
        }
    }
}
