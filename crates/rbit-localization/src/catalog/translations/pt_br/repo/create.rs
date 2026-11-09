//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("Novo repositório"),
        RepoCreateLabel::RepositoryType => text.literal("Tipo de repositório"),
        RepoCreateLabel::Empty => text.literal("Vazio"),
        RepoCreateLabel::Mirror => text.literal("Espelho"),
        RepoCreateLabel::Import => text.literal("Importar"),
        RepoCreateLabel::NewLead => {
            text.literal(
                "Crie um repositório vazio, importe um projeto ou mantenha um espelho sincronizado com a origem.",
            );
        }
        RepoCreateLabel::Owner => text.literal("Proprietário"),
        RepoCreateLabel::Name => text.literal("Nome"),
        RepoCreateLabel::Description => text.literal("Descrição"),
        RepoCreateLabel::Optional => text.literal("opcional"),
        RepoCreateLabel::Visibility => text.literal("Visibilidade"),
        RepoCreateLabel::Public => text.literal("Público"),
        RepoCreateLabel::Private => text.literal("Privado"),
        RepoCreateLabel::PublicRepository => text.literal("Qualquer pessoa pode ver e clonar este repositório."),
        RepoCreateLabel::PublicMirror => text.literal("Qualquer pessoa pode ver e clonar o espelho."),
        RepoCreateLabel::PublicImport => text.literal("Qualquer pessoa pode ver e clonar o repositório."),
        RepoCreateLabel::PrivateHint => {
            text.literal("Somente o proprietário pode vê-lo. Em uma organização, isso significa os membros dela.");
        }
        RepoCreateLabel::Cancel => text.literal("Cancelar"),
        RepoCreateLabel::CreateRepository => text.literal("Criar repositório"),
        RepoCreateLabel::MirrorLead => {
            text.literal(
                "Um espelho busca periodicamente os branches e tags de um repositório público hospedado em outro \
                 lugar. Aqui ele é somente leitura: as alterações vêm do upstream.",
            );
        }
        RepoCreateLabel::MirrorsDisabled => text.literal("Os espelhos estão desativados"),
        RepoCreateLabel::MirrorsDisabledText => {
            text.literal("Um administrador desta instância desativou os espelhos de pull.");
        }
        RepoCreateLabel::UpstreamUrl => text.literal("URL do upstream"),
        RepoCreateLabel::UrlHintStart => text.literal("Uma URL"),
        RepoCreateLabel::UrlHintOr => text.literal("ou"),
        RepoCreateLabel::UrlHintEnd => text.literal("de um repositório público."),
        RepoCreateLabel::FromTheUrl => text.literal("a partir da URL"),
        RepoCreateLabel::FetchEvery => text.literal("Buscar a cada"),
        RepoCreateLabel::Minutes => text.literal("minutos"),
        RepoCreateLabel::CreateMirror => text.literal("Criar espelho"),
        RepoCreateLabel::MirrorARepository => text.literal("Espelhar um repositório"),
        RepoCreateLabel::ImportLead => {
            text.literal(
                "Copia um repositório do GitHub ou do Gitea com o histórico e, se você quiser, as issues, pull \
                 requests, etiquetas, marcos, releases e arquivos Git LFS. Os números de issues e pull requests \
                 continuam os mesmos.",
            );
        }
        RepoCreateLabel::ImportingDisabled => text.literal("A importação está desativada"),
        RepoCreateLabel::ImportingDisabledText => {
            text.literal("Um administrador desta instância desativou a importação.");
        }
        RepoCreateLabel::ImportFrom => text.literal("Importar de"),
        RepoCreateLabel::ServerAddress => text.literal("Endereço do servidor"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("desnecessário para github.com"),
        RepoCreateLabel::Repository => text.literal("Repositório"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("proprietário/nome ou a URL"),
        RepoCreateLabel::AccessToken => text.literal("Token de acesso"),
        RepoCreateLabel::OptionalForPublic => text.literal("opcional para repositórios públicos"),
        RepoCreateLabel::TokenHint => {
            text.literal(
                "Um token que pode ler o repositório aumenta os limites de requisições da origem e é necessário para \
                 repositórios privados. Ele é usado apenas durante a importação e nunca é armazenado.",
            );
        }
        RepoCreateLabel::AsOnTheSource => text.literal("igual ao da origem"),
        RepoCreateLabel::BringAlong => text.literal("Trazer junto"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("Sem suporte: esta instância não tem wikis."),
        RepoCreateLabel::StartImport => text.literal("Iniciar importação"),
        RepoCreateLabel::ImportARepository => text.literal("Importar um repositório"),
        RepoCreateLabel::ImportTitle => text.literal("Importação"),
        RepoCreateLabel::FailedNote => text.literal("Tudo o que foi importado até então está no repositório."),
        RepoCreateLabel::NotReached => text.literal("Não alcançado"),
        RepoCreateLabel::Started => text.literal("Iniciada"),
        RepoCreateLabel::WaitingForSlot => text.literal("Aguardando uma vaga livre"),
        RepoCreateLabel::Ended => text.literal("Terminada"),
        RepoCreateLabel::UpdatesByItself => text.literal("Esta página se atualiza sozinha."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("Ignorados ou incompletos"),
        RepoCreateLabel::GoToRepository => text.literal("Ir para o repositório"),
        RepoCreateLabel::OpensAutomatically => {
            text.literal("O repositório será aberto automaticamente quando a primeira busca do espelho terminar.");
        }
        RepoCreateLabel::FetchInProgress => text.literal("Busca do espelho em andamento"),
        RepoCreateLabel::TransferSizeNote => {
            text.literal(
                "O tamanho da transferência é determinado pelo upstream. Esta página mostra a etapa atual conforme o \
                 trabalho avança.",
            );
        }
        RepoCreateLabel::RefreshStatus => text.literal("Atualizar status"),
        RepoCreateLabel::SettingsAndRetry => text.literal("Configurações do espelho e nova tentativa"),
        RepoCreateLabel::NoScriptHint => {
            text.literal("Use Atualizar status para acompanhar o progresso e abra o repositório quando estiver pronto.");
        }
        RepoCreateLabel::Connecting => text.literal("Conectando ao repositório upstream."),
        RepoCreateLabel::Fetching => {
            text.literal(
                "Baixando branches, tags e objetos Git. Repositórios grandes podem levar vários minutos.",
            );
        }
        RepoCreateLabel::Finalizing => text.literal("Preparando o conteúdo do repositório e o branch padrão."),
        RepoCreateLabel::FetchFailedRetry => {
            text.literal(
                "Não foi possível buscar o espelho. Um administrador do repositório pode tentar novamente nas \
                 configurações do espelho.",
            );
        }
        RepoCreateLabel::Complete => text.literal("Espelhamento concluído. Abrindo o repositório…"),
        RepoCreateLabel::Queued => text.literal("Aguardando um processo de espelhamento disponível."),
        RepoCreateLabel::Stopped => text.literal("O espelhamento parou. Abrindo o repositório…"),
        RepoCreateLabel::Mirroring => text.literal("Espelhando"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("Pelo menos ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("De ");
            text.argument(source);
            text.literal(":");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("e mais ");
            text.number(count);
            text.literal(" (veja o log do servidor)");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.literal("Espelhando ");
            text.argument(name);
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("Não foi possível buscar o espelho: ");
            text.argument(error);
        }
    }
}
