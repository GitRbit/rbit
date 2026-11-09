//! Brazilian Portuguese (`pt-BR`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;
use crate::plural::Plural;

entries! { PtBr:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("Páginas"),
        WikiMessage::AllPages => text.literal("Todas as páginas"),
        WikiMessage::PagesLabel => text.literal("Páginas da wiki"),
        WikiMessage::BreadcrumbLabel => text.literal("Trilha de navegação"),
        WikiMessage::SearchPages => text.literal("Pesquisar páginas"),
        WikiMessage::Search => text.literal("Pesquisar"),
        WikiMessage::NewPage => text.literal("Nova página"),
        WikiMessage::EditPage => text.literal("Editar página"),
        WikiMessage::Edit => text.literal("Editar"),
        WikiMessage::History => text.literal("Histórico"),
        WikiMessage::Delete => text.literal("Excluir"),
        WikiMessage::LastEdited => text.literal("Última edição"),
        WikiMessage::Revision { generation } => {
            text.literal("Revisão ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("Revisão"),
        WikiMessage::Author => text.literal("Autor"),
        WikiMessage::Date => text.literal("Data"),
        WikiMessage::UnknownAuthor => text.literal("Autor desconhecido"),
        WikiMessage::Created => text.literal("Criada"),
        WikiMessage::Edited => text.literal("Editada"),
        WikiMessage::Renamed => text.literal("Renomeada"),
        WikiMessage::Deleted => text.literal("Excluída"),
        WikiMessage::Restored => text.literal("Restaurada"),
        WikiMessage::View => text.literal("Ver"),
        WikiMessage::CompareWithPrevious => text.literal("Comparar com a anterior"),
        WikiMessage::Restore => text.literal("Restaurar esta revisão"),
        WikiMessage::OlderRevisions => text.literal("Revisões anteriores"),
        WikiMessage::OldRevisionNotice => text.literal("Você está vendo uma revisão anterior desta página."),
        WikiMessage::ViewCurrent => text.literal("Ver a página atual"),
        WikiMessage::HistoryOf { title } => {
            text.literal("Histórico de ");
            text.argument(title);
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("Alterações da revisão ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal(" para a revisão ");
            text.number(u64::try_from(head).unwrap_or_default());
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(match text.plural(additions) {
                Plural::One => " adição, ",
                _ => " adições, ",
            });
            text.number(deletions);
            text.literal(match text.plural(deletions) {
                Plural::One => " remoção",
                _ => " remoções",
            });
        }
        WikiMessage::NoDifferences => text.literal("Estas revisões têm o mesmo conteúdo."),
        WikiMessage::Title => text.literal("Título"),
        WikiMessage::Path => text.literal("Caminho"),
        WikiMessage::PathHint => {
            text.literal("Letras, dígitos, - _ e . separados por /. Deixe vazio para derivá-lo do título.");
        }
        WikiMessage::Content => text.literal("Conteúdo"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("Salvar página"),
        WikiMessage::Preview => text.literal("Pré-visualizar"),
        WikiMessage::NothingToPreview => text.literal("Nada para pré-visualizar."),
        WikiMessage::Cancel => text.literal("Cancelar"),
        WikiMessage::TitleEmpty => text.literal("Digite um título."),
        WikiMessage::TitleTooLong => text.literal("O título é longo demais."),
        WikiMessage::TitleInvalid => text.literal("O título contém caracteres não permitidos."),
        WikiMessage::PathEmpty => text.literal("Digite um caminho ou um título do qual ele possa ser derivado."),
        WikiMessage::PathTooLong => text.literal("O caminho ou uma de suas partes é longo demais."),
        WikiMessage::PathTooDeep => text.literal("O caminho tem partes demais."),
        WikiMessage::PathInvalid => {
            text.literal(
                "Cada parte do caminho deve começar com uma letra ou dígito e usar apenas letras, dígitos, - _ e .",
            );
        }
        WikiMessage::PathReserved => text.literal("O caminho usa um nome reservado."),
        WikiMessage::ContentTooLarge => text.literal("O conteúdo é grande demais."),
        WikiMessage::ContentInvalid => text.literal("O conteúdo contém caracteres não permitidos."),
        WikiMessage::Conflict => {
            text.literal(
                "Esta página mudou depois que você começou. Seu texto foi mantido abaixo; revise a página atual antes \
                 de salvar novamente.",
            );
        }
        WikiMessage::CompareChanges => text.literal("Comparar as alterações"),
        WikiMessage::PathTaken => text.literal("Outra página já usa este caminho."),
        WikiMessage::Unchanged => text.literal("Nada mudou."),
        WikiMessage::DeleteTitle { title } => {
            text.literal("Excluir ");
            text.argument(title);
            text.literal("?");
        }
        WikiMessage::DeletePage => text.literal("Excluir página"),
        WikiMessage::Disabled => text.literal("A wiki está desativada neste repositório."),
        WikiMessage::About => {
            text.literal(
                "Uma wiki reúne páginas de documentação com histórico próprio, separadas do código.",
            );
        }
        WikiMessage::TurnOn => text.literal("Ativar a wiki"),
        WikiMessage::TurnOff => text.literal("Desativar a wiki"),
        WikiMessage::Empty => text.literal("Esta wiki ainda não tem páginas."),
        WikiMessage::CreateFirst => text.literal("Criar a primeira página"),
        WikiMessage::Missing => text.literal("Nenhuma página tem este caminho."),
        WikiMessage::CreatePath { path } => {
            text.literal("Criar ");
            text.argument(path);
        }
        WikiMessage::SearchCount { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " página corresponde a “",
                _ => " páginas correspondem a “",
            });
            text.argument(query);
            text.literal("”");
        }
        WikiMessage::NoResults { query } => {
            text.literal("Nenhuma página corresponde a “");
            text.argument(query);
            text.literal("”.");
        }
        WikiMessage::MorePages => text.literal("Mais páginas"),
        WikiMessage::DeleteKeepsHistory => text.literal(
            "A página é removida da wiki. O histórico dela é mantido, e quem tem acesso de escrita pode              restaurá-la a partir das páginas excluídas.",
        ),
        WikiMessage::DeletedPages => text.literal("Páginas excluídas"),
        WikiMessage::NoDeletedPages => text.literal("Nenhuma página foi excluída."),
        WikiMessage::DeletedNotice => text.literal("Esta página foi excluída. O histórico dela é mantido."),
        WikiMessage::RestorePage => text.literal("Restaurar página"),
        WikiMessage::MoreDeletedPages => text.literal("Exclusões anteriores"),
        WikiMessage::RestoreBlocked => text.literal(
            "Outra página agora usa este caminho. Renomeie essa página primeiro e depois restaure esta.",
        ),
        WikiMessage::RenamedFrom { path } => {
            text.literal("Renomeada de ");
            text.argument(path);
        }
    }
}
