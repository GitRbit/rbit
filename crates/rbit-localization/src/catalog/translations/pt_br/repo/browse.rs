//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/browse.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("Sobre"),
        RepoBrowseLabel::NoDescription => text.literal("Sem descrição."),
        RepoBrowseLabel::Visibility => text.literal("Visibilidade"),
        RepoBrowseLabel::History => text.literal("Histórico"),
        RepoBrowseLabel::CommitHistory => text.literal("Histórico de commits"),
        RepoBrowseLabel::AddFile => text.literal("Adicionar arquivo"),
        RepoBrowseLabel::UploadFiles => text.literal("Enviar arquivos"),
        RepoBrowseLabel::Files => text.literal("Arquivos"),
        RepoBrowseLabel::ParentDirectory => text.literal("Diretório pai"),
        RepoBrowseLabel::Path => text.literal("Caminho"),
        RepoBrowseLabel::Latest => text.literal("Mais recente"),
        RepoBrowseLabel::YourAccess => text.literal("Seu acesso"),
        RepoBrowseLabel::LeaveRepository => text.literal("Sair do repositório"),
        RepoBrowseLabel::Languages => text.literal("Linguagens"),
        RepoBrowseLabel::RepositoryLinks => text.literal("Links do repositório"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Armazenado com Git LFS"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("O objeto não está neste servidor"),
        RepoBrowseLabel::EditThisFile => text.literal("Editar este arquivo"),
        RepoBrowseLabel::DeleteThisFile => text.literal("Excluir este arquivo"),
        RepoBrowseLabel::Blame => text.literal("Blame"),
        RepoBrowseLabel::Raw => text.literal("Bruto"),
        RepoBrowseLabel::Download => text.literal("Baixar"),
        RepoBrowseLabel::BinaryNotShown => text.literal("Arquivo binário não exibido."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("Este arquivo é grande demais para ser exibido."),
        RepoBrowseLabel::ViewRaw => text.literal("Ver bruto"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("Documentos do repositório"),
        RepoBrowseLabel::DocumentOutline => text.literal("Estrutura do documento"),
        RepoBrowseLabel::FilterHeadings => text.literal("Filtrar títulos"),
        RepoBrowseLabel::DocumentHeadings => text.literal("Títulos do documento"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("Nenhum título corresponde."),
        RepoBrowseLabel::EmptyTitle => text.literal("Este repositório está vazio"),
        RepoBrowseLabel::EmptyLead => {
            text.literal("Faça push de um projeto existente ou comece um novo pela linha de comando.");
        }
        RepoBrowseLabel::RepositoryUrl => text.literal("URL do repositório"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("Copiar URL do repositório"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("URL SSH do repositório"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("Copiar URL SSH do repositório"),
        RepoBrowseLabel::EmptySshNote => {
            text.literal(
                "Os comandos abaixo usam HTTPS. Para fazer push por SSH, use a URL SSH e adicione uma chave em ",
            );
        }
        RepoBrowseLabel::CreateNewRepository => text.literal("Criar um novo repositório"),
        RepoBrowseLabel::PushExistingRepository => text.literal("Fazer push de um repositório existente"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("Fechar o navegador de arquivos"),
        RepoBrowseLabel::LanguageFiles => text.literal("Arquivos da linguagem"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("Nenhum arquivo encontrado para esta linguagem."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("Nenhum arquivo corresponde."),
        RepoBrowseLabel::FileContent => text.literal("Conteúdo do arquivo"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("Selecione um arquivo para pré-visualizar o conteúdo."),
        RepoBrowseLabel::SearchThisListing => text.literal("Pesquisar nesta listagem"),
        RepoBrowseLabel::SearchNames => text.literal("Pesquisar nomes"),
        RepoBrowseLabel::Search => text.literal("Pesquisar"),
        RepoBrowseLabel::ListingPages => text.literal("Páginas da listagem"),
        RepoBrowseLabel::Previous => text.literal("Anterior"),
        RepoBrowseLabel::Next => text.literal("Próxima"),
        RepoBrowseLabel::BranchesAndTags => text.literal("Branches e tags"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("Navegar por branches e tags"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("Encontrar um branch ou tag"),
        RepoBrowseLabel::Find => text.literal("Encontrar"),
        RepoBrowseLabel::Branch => text.literal("Branch"),
        RepoBrowseLabel::Tag => text.literal("Tag"),
        RepoBrowseLabel::Default => text.literal("padrão"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("Nenhum branch ou tag corresponde."),
        RepoBrowseLabel::NoForksYet => text.literal("Ainda não há forks"),
        RepoBrowseLabel::ForksEmpty => {
            text.literal("Forks são cópias deste repositório em que outras pessoas trabalham de forma independente.");
        }
        RepoBrowseLabel::ForkLead => {
            text.literal(
                "Um fork é a sua própria cópia dos branches e tags do repositório. Ele mantém a visibilidade do original.",
            );
        }
        RepoBrowseLabel::ForkLeadPrivate => {
            text.literal(
                "Um fork é a sua própria cópia dos branches e tags do repositório. Ele mantém a visibilidade do \
                 original — este continuará privado.",
            );
        }
        RepoBrowseLabel::Owner => text.literal("Proprietário"),
        RepoBrowseLabel::Name => text.literal("Nome"),
        RepoBrowseLabel::CreateFork => text.literal("Criar fork"),
        RepoBrowseLabel::Cancel => text.literal("Cancelar"),
        RepoBrowseLabel::NoStargazersYet => text.literal("Ninguém marcou com estrela ainda"),
        RepoBrowseLabel::StargazersEmpty => {
            text.literal("As pessoas que marcarem este repositório com estrela aparecerão aqui.");
        }
        RepoBrowseLabel::RepositoryActivity => text.literal("Atividade do repositório"),
        RepoBrowseLabel::GoToFile => text.literal("Ir para arquivo"),
        RepoBrowseLabel::MatchingFiles => text.literal("Arquivos correspondentes"),
        RepoBrowseLabel::LoadingFiles => text.literal("Carregando arquivos…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("Não foi possível carregar os arquivos. Pressione Enter para pesquisar."),
        RepoBrowseLabel::SeeAllMatches => text.literal("Ver todas as correspondências"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("Esta revisão não tem arquivos."),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("Esta revisão tem arquivos demais para pesquisar em todos."),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "branch",
                _ => "branches",
            });
        }
        RepoBrowseMessage::TagNoun { count } => {
            text.literal(match text.plural(count as u64) {
                Plural::One => "tag",
                _ => "tags",
            });
        }
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("Acesso de ");
            text.argument(permission);
            text.literal(" como colaborador.");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " linha",
                _ => " linhas",
            });
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.literal("Arquivos ");
            text.argument(language);
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " resultado · ",
                _ => " resultados · ",
            });
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · ");
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " ref",
                _ => " refs",
            });
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.literal("Fazer fork de ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        RepoBrowseMessage::CommitNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "commit",
            _ => "commits",
        }),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("Nenhum arquivo corresponde a “");
            text.argument(query);
            text.literal("”.");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.literal("Correspondências adicionais: ");
            text.number(count as u64);
            text.literal(". Digite mais do nome para restringir a lista.");
        }
    }
}
