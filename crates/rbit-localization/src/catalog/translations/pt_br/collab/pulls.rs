//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("Pull requests"),
        PullMessage::SearchPulls => text.literal("Pesquisar pull requests"),
        PullMessage::ClearSearch => text.literal("Limpar a pesquisa e os filtros"),
        PullMessage::FiltersLabel => text.literal("Filtros de pull requests"),
        PullMessage::StateNav => text.literal("Estado do pull request"),
        PullMessage::AuthorFilter { author } => {
            text.literal("autor: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("Novo"),
        PullMessage::NewSuffix => text.literal("\u{a0}pull request"),
        PullMessage::EmptyTitle => text.literal("Nenhum pull request aqui"),
        PullMessage::EmptyText => {
            text.literal(
                "Pull requests propõem alterações de um branch ou de um fork. Compare branches para abrir um.",
            );
        }
        PullMessage::OpenTitle => text.literal("Abrir um pull request"),
        PullMessage::OpenIntro => {
            text.literal(
                "Proponha as alterações de um branch, deste repositório ou de um fork, para o branch base.",
            );
        }
        PullMessage::OpenAsDraft => text.literal("Abrir como rascunho"),
        PullMessage::Create => text.literal("Criar pull request"),
        PullMessage::DifferentHistories => text.literal("Estes branches têm históricos totalmente diferentes."),
        PullMessage::ChooseBranches => text.literal("Escolha dois branches existentes para comparar."),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" já tem todos os commits de ");
            text.argument(head);
            text.literal(".");
        }
        PullMessage::BaseRepository => text.literal("Repositório base"),
        PullMessage::BaseRefs => text.literal("Branches e tags base"),
        PullMessage::Base => text.literal("Base"),
        PullMessage::HeadRepository => text.literal("Repositório de origem"),
        PullMessage::Compare => text.literal("Comparar"),
        PullMessage::CompareRefs => text.literal("Comparar branches e tags"),
        PullMessage::PullRequest => text.literal("Pull request"),
        PullMessage::Conversation => text.literal("Conversa"),
        PullMessage::Commits => text.literal("Commits"),
        PullMessage::FilesChanged => text.literal("Arquivos alterados"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commit",
                _ => " commits",
            });
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("commits"),
        PullMessage::ChangeBase => text.literal("Alterar o branch base"),
        PullMessage::BaseBranch => text.literal("Branch base"),
        PullMessage::BaseBranches => text.literal("Branches base"),
        PullMessage::ChangeBaseHint => {
            text.literal("Alterar a base atualiza os commits, o diff e as verificações de mesclagem.");
        }
        PullMessage::UpdateBase => text.literal("Atualizar o branch base"),
        PullMessage::IsClosed => text.literal("Este pull request está fechado."),
        PullMessage::IsMerged => text.literal("Pull request mesclado."),
        PullMessage::DeleteBranch { branch } => {
            text.literal("Excluir ");
            text.argument(branch);
        }
        PullMessage::ConvertToDraft => text.literal("Converter em rascunho"),
        PullMessage::NoCommitsTitle => text.literal("Nenhum commit"),
        PullMessage::NoCommitsText => text.literal("A origem e a base apontam para o mesmo commit."),
        PullMessage::NewestCommits { count } => {
            let one = text.plural(count) == Plural::One;
            text.literal(if one { "Apenas " } else { "Apenas os " });
            text.number(count);
            text.literal(if one { " commit mais recente é listado." } else { " commits mais recentes são listados." });
        }
    }
}
