//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("Navegar pelos arquivos neste commit"),
        RepoHistoryLabel::BrowseFiles => text.literal("Navegar pelos arquivos"),
        RepoHistoryLabel::Pagination => text.literal("Paginação"),
        RepoHistoryLabel::Newer => text.literal("Mais recentes"),
        RepoHistoryLabel::Older => text.literal("Mais antigos"),
        RepoHistoryLabel::Committed => text.literal("fez commit"),
        RepoHistoryLabel::CommittedBy => text.literal("commit feito por"),
        RepoHistoryLabel::Commit => text.literal("commit"),
        RepoHistoryLabel::Checks => text.literal("Verificações"),
        RepoHistoryLabel::ViewFile => text.literal("Ver arquivo"),
        RepoHistoryLabel::PathInThatCommit => text.literal("Caminho naquele commit"),
        RepoHistoryLabel::FileEmpty => text.literal("Este arquivo está vazio."),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("Não é possível fazer blame de arquivos binários."),
        RepoHistoryLabel::TooLargeToBlame => text.literal("Este arquivo é grande demais para o blame."),
        RepoHistoryLabel::ViewTheFile => text.literal("Ver o arquivo"),
        RepoHistoryLabel::CompareChanges => text.literal("Comparar alterações"),
        RepoHistoryLabel::CompareLead => {
            text.literal(
                "Escolha uma base e o branch, tag ou commit a comparar com ela, inclusive entre forks.",
            );
        }
        RepoHistoryLabel::Comparison => text.literal("Comparação"),
        RepoHistoryLabel::MergeBase => text.literal("base de mesclagem"),
        RepoHistoryLabel::Swap => text.literal("Inverter"),
        RepoHistoryLabel::CreatePullRequest => text.literal("Criar pull request"),
        RepoHistoryLabel::UnrelatedHistories => {
            text.literal("Estas revisões têm históricos totalmente diferentes, então não há diff para mostrar.");
        }
        RepoHistoryLabel::NothingToCompare => text.literal("Nada para comparar"),
        RepoHistoryLabel::NothingToCompareText => {
            text.literal("Tudo o que está na revisão comparada já está na base.");
        }
        RepoHistoryLabel::Commits => text.literal("Commits"),
        RepoHistoryLabel::Compare => text.literal("Comparar"),
        RepoHistoryLabel::CompareWithDefault => text.literal("Comparar com o branch padrão"),
        RepoHistoryLabel::BranchView => text.literal("Visualização de branches"),
        RepoHistoryLabel::List => text.literal("Lista"),
        RepoHistoryLabel::GitTree => text.literal("Árvore Git"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("Nenhuma ref corresponde."),
        RepoHistoryLabel::NoBranchesYet => text.literal("Ainda não há branches"),
        RepoHistoryLabel::NoBranchesText => text.literal("Faça push de um commit para criar o primeiro branch."),
        RepoHistoryLabel::NoTagsYet => text.literal("Ainda não há tags"),
        RepoHistoryLabel::NoTagsText => {
            text.literal("As tags marcam releases e outros pontos importantes do histórico.");
        }
        RepoHistoryLabel::Protected => text.literal("Protegido"),
        RepoHistoryLabel::ProtectedTitle => text.literal("Uma regra de proteção de branch se aplica a este branch"),
        RepoHistoryLabel::DownloadTarGz => text.literal("Baixar tar.gz"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("Gráfico de commits dos branches"),
        RepoHistoryLabel::GraphHelp => {
            text.literal(
                "Siga as linhas coloridas para rastrear o histórico. Selecione uma trilha para destacar os commits dela.",
            );
        }
        RepoHistoryLabel::ClearHighlight => text.literal("Limpar destaque"),
        RepoHistoryLabel::HistoryConnections => text.literal("Conexões do histórico"),
        RepoHistoryLabel::CommitDetails => text.literal("Detalhes do commit"),
        RepoHistoryLabel::Merge => text.literal("Mesclagem"),
        RepoHistoryLabel::GraphTruncated => {
            text.literal(
                "Mostrando os 200 commits mais recentes de todos os branches. As linhas na parte inferior continuam \
                 no histórico mais antigo. Abra o histórico de um branch para explorar mais.",
            );
        }
        RepoHistoryLabel::LinesAdded => text.literal("Adicionadas"),
        RepoHistoryLabel::LinesDeleted => text.literal("Removidas"),
        RepoHistoryLabel::StatusAdded => text.literal("Adicionado"),
        RepoHistoryLabel::StatusDeleted => text.literal("Excluído"),
        RepoHistoryLabel::StatusModified => text.literal("Modificado"),
        RepoHistoryLabel::StatusRenamed => text.literal("Renomeado"),
        RepoHistoryLabel::StatusCopied => text.literal("Copiado"),
        RepoHistoryLabel::ChangedFiles => text.literal("Arquivos alterados"),
        RepoHistoryLabel::FileTree => text.literal("Árvore de arquivos"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("Filtrar arquivos alterados"),
        RepoHistoryLabel::FilterFiles => text.literal("Filtrar arquivos…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("Nenhum arquivo corresponde."),
        RepoHistoryLabel::Changes => text.literal("Alterações"),
        RepoHistoryLabel::ToggleFileTree => text.literal("Mostrar ou ocultar a árvore de arquivos"),
        RepoHistoryLabel::CollapseAll => text.literal("Recolher tudo"),
        RepoHistoryLabel::ExpandAll => text.literal("Expandir tudo"),
        RepoHistoryLabel::DiffTruncated => {
            text.literal(
                "Esta alteração é muito grande, então alguns diffs não são mostrados. Clone o repositório para ver a \
                 alteração completa.",
            );
        }
        RepoHistoryLabel::CollapseHint => {
            text.literal("Clique para recolher ou expandir · Alt-clique para todos os arquivos");
        }
        RepoHistoryLabel::CopyPath => text.literal("Copiar caminho"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("Ver o arquivo neste commit"),
        RepoHistoryLabel::View => text.literal("Ver"),
        RepoHistoryLabel::BinaryNotShown => text.literal("Arquivo binário não exibido."),
        RepoHistoryLabel::DiffTooLarge => text.literal("Diff grande demais para ser exibido."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("Commit de submódulo"),
        RepoHistoryLabel::NoContentChanges => text.literal("Nenhuma alteração de conteúdo."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.literal("Commits em ");
            text.argument(date);
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("Ver commit ");
            text.argument(id);
        }
        RepoHistoryMessage::ParentNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "pai",
                _ => "pais",
            });
        }
        RepoHistoryMessage::BlameOf { file } => {
            text.literal("Blame de ");
            text.argument(file);
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("Blame de ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" à frente");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" atrás");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            let one = text.plural(count as u64) == Plural::One;
            text.literal(if one { "Apenas " } else { "Apenas os " });
            text.number(count as u64);
            text.literal(if one { " commit mais recente é listado." } else { " commits mais recentes são listados." });
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("Destacar trilha ");
            text.number(track as u64);
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("Trilha ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("Branch: ");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count } => {
            text.literal("mais ");
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " branch",
                _ => " branches",
            });
        }
        RepoHistoryMessage::FilesChangedNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "arquivo alterado",
                _ => "arquivos alterados",
            });
        }
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("Comparar ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
