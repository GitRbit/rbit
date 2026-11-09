//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(match menu {
            IssueFilterMenu::State => "Estado",
            IssueFilterMenu::Status => "Status",
            IssueFilterMenu::Author => "Autor",
            IssueFilterMenu::Label => "Etiqueta",
            IssueFilterMenu::Milestone => "Marco",
            IssueFilterMenu::Assignee => "Responsável",
            IssueFilterMenu::Sort => "Ordenar",
        }),
        IssueFilterMessage::Find(menu) => {
            text.literal("Encontrar ");
            text.literal(noun(menu));
        }
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal("Encontrar ");
            text.literal(noun(menu));
            text.literal("…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("Ainda não há nada para escolher."),
        IssueFilterMessage::NoMatches => text.literal("Nenhum resultado."),
        IssueFilterMessage::Anyone => text.literal("Qualquer pessoa"),
        IssueFilterMessage::Nobody => text.literal("Ninguém"),
        IssueFilterMessage::You => text.literal("Você"),
        IssueFilterMessage::Any => text.literal("Qualquer"),
        IssueFilterMessage::NoMilestone => text.literal("Sem marco"),
        IssueFilterMessage::AnyLabel => text.literal("Qualquer etiqueta"),
        IssueFilterMessage::NoLabels => text.literal("Sem etiquetas"),
        IssueFilterMessage::Open => text.literal("Abertos"),
        IssueFilterMessage::Closed => text.literal("Fechados"),
        IssueFilterMessage::AllStates => text.literal("Todos os estados"),
        IssueFilterMessage::AnyStatus => text.literal("Qualquer status"),
        IssueFilterMessage::Draft => text.literal("Rascunho"),
        IssueFilterMessage::Merged => text.literal("Mesclados"),
        IssueFilterMessage::SortNewest => text.literal("Mais recentes"),
        IssueFilterMessage::SortOldest => text.literal("Mais antigos"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("Atualizados recentemente"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("Atualizados há mais tempo"),
        IssueFilterMessage::SortMostCommented => text.literal("Mais comentados"),
    }
}

/// The menu's noun after “Encontrar”.
const fn noun(menu: IssueFilterMenu) -> &'static str {
    match menu {
        IssueFilterMenu::State => "estado",
        IssueFilterMenu::Status => "status",
        IssueFilterMenu::Author => "autor",
        IssueFilterMenu::Label => "etiqueta",
        IssueFilterMenu::Milestone => "marco",
        IssueFilterMenu::Assignee => "responsável",
        IssueFilterMenu::Sort => "ordenação",
    }
}
