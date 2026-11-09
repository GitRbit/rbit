//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("Configurações"),
        RepoSettingsLabel::AdminsOf => text.literal("Somente administradores de"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("podem ver esta página."),
        RepoSettingsLabel::SettingsSaved => text.literal("Configurações salvas."),
        RepoSettingsLabel::General => text.literal("Geral"),
        RepoSettingsLabel::Description => text.literal("Descrição"),
        RepoSettingsLabel::Optional => text.literal("opcional"),
        RepoSettingsLabel::Visibility => text.literal("Visibilidade"),
        RepoSettingsLabel::Public => text.literal("Público"),
        RepoSettingsLabel::PublicHint => text.literal("Qualquer pessoa pode ver e clonar este repositório."),
        RepoSettingsLabel::Private => text.literal("Privado"),
        RepoSettingsLabel::PrivateHint => text.literal("Somente pessoas com acesso podem vê-lo e cloná-lo."),
        RepoSettingsLabel::DefaultBranch => text.literal("Branch padrão"),
        RepoSettingsLabel::PushABranch => text.literal("Faça push de um branch para escolher o branch padrão."),
        RepoSettingsLabel::DefaultBranchHint => {
            text.literal("Mostrado primeiro na navegação e usado no checkout do git clone.");
        }
        RepoSettingsLabel::SaveChanges => text.literal("Salvar alterações"),
        RepoSettingsLabel::RepositoryName => text.literal("Nome do repositório"),
        RepoSettingsLabel::Name => text.literal("Nome"),
        RepoSettingsLabel::RenameHint => {
            text.literal("Links e URLs de clone que usam o nome antigo deixarão de funcionar.");
        }
        RepoSettingsLabel::Rename => text.literal("Renomear"),
        RepoSettingsLabel::DangerZone => text.literal("Zona de perigo"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("Desarquivar este repositório"),
        RepoSettingsLabel::UnarchiveHint => {
            text.literal(
                "Ele volta a aceitar pushes, issues, pull requests, comentários e releases, e os espelhos dele são \
                 retomados.",
            );
        }
        RepoSettingsLabel::Unarchive => text.literal("Desarquivar repositório"),
        RepoSettingsLabel::ArchiveTitle => text.literal("Arquivar este repositório"),
        RepoSettingsLabel::ArchiveHint => {
            text.literal(
                "Ele se torna somente leitura: as pessoas ainda podem cloná-lo, fazer fork e marcá-lo com estrela, mas \
                 ninguém pode fazer push, mesclar nem alterar as issues, pull requests ou releases dele, e os espelhos \
                 dele são pausados. Você pode desarquivá-lo a qualquer momento.",
            );
        }
        RepoSettingsLabel::Archive => text.literal("Arquivar repositório"),
        RepoSettingsLabel::DeleteTitle => text.literal("Excluir este repositório"),
        RepoSettingsLabel::DeleteHint => {
            text.literal(
                "Os arquivos, o histórico e as estrelas dele serão removidos permanentemente. Isso não pode ser \
                 desfeito.",
            );
        }
        RepoSettingsLabel::Type => text.literal("Digite"),
        RepoSettingsLabel::ToConfirm => text.literal("para confirmar"),
        RepoSettingsLabel::DeleteRepository => text.literal("Excluir repositório"),
        RepoSettingsLabel::DeployKeys => text.literal("Chaves de implantação"),
        RepoSettingsLabel::DeployKeysHint => {
            text.literal("Chaves que dão a um servidor ou job de CI acesso SSH somente a este repositório.");
        }
        RepoSettingsLabel::DeployKeysLeadStart => {
            text.literal("Uma chave de implantação dá a um servidor ou job de CI acesso SSH a");
        }
        RepoSettingsLabel::DeployKeysLeadEnd => {
            text.literal("e a nada mais. As chaves são somente leitura, a menos que você permita que elas façam push.");
        }
        RepoSettingsLabel::NewDeployKey => text.literal("Nova chave de implantação"),
        RepoSettingsLabel::Title => text.literal("Título"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("ex.: Servidor de produção"),
        RepoSettingsLabel::TitleHint => text.literal("Por padrão, o comentário da chave."),
        RepoSettingsLabel::Key => text.literal("Chave"),
        RepoSettingsLabel::KeyHint => {
            text.literal(
                "Uma chave pode pertencer a apenas um repositório ou conta; gere uma chave separada para cada um.",
            );
        }
        RepoSettingsLabel::AllowWrite => text.literal("Permitir acesso de escrita"),
        RepoSettingsLabel::AllowWriteHint => {
            text.literal("A chave pode fazer push para este repositório, além de cloná-lo.");
        }
        RepoSettingsLabel::AddDeployKey => text.literal("Adicionar chave de implantação"),
        RepoSettingsLabel::Keys => text.literal("Chaves"),
        RepoSettingsLabel::NoDeployKeys => text.literal("Nenhuma chave de implantação"),
        RepoSettingsLabel::NoDeployKeysText => {
            text.literal(
                "Adicione uma chave para permitir que um servidor ou job de CI clone este repositório por SSH.",
            );
        }
        RepoSettingsLabel::Delete => text.literal("Excluir"),
        RepoSettingsLabel::Mirror => text.literal("Espelho"),
        RepoSettingsLabel::MirrorHint => {
            text.literal("Upstream, intervalo de busca e o resultado da última busca.");
        }
        RepoSettingsLabel::MirrorLead => {
            text.literal("Este repositório busca os branches e tags do upstream e não aceita pushes.");
        }
        RepoSettingsLabel::Upstream => text.literal("Upstream"),
        RepoSettingsLabel::LastFetch => text.literal("Última busca"),
        RepoSettingsLabel::Failed => text.literal("Falhou"),
        RepoSettingsLabel::Ok => text.literal("OK"),
        RepoSettingsLabel::NotYet => text.literal("Ainda não"),
        RepoSettingsLabel::NextFetch => text.literal("Próxima busca"),
        RepoSettingsLabel::Error => text.literal("Erro"),
        RepoSettingsLabel::FetchNow => text.literal("Buscar agora"),
        RepoSettingsLabel::FetchEvery => text.literal("Buscar a cada"),
        RepoSettingsLabel::Minutes => text.literal("minutos"),
        RepoSettingsLabel::Save => text.literal("Salvar"),
        RepoSettingsLabel::Saved => text.literal("Salvo."),
        RepoSettingsLabel::FetchQueued => {
            text.literal("Uma busca está na fila; atualize em instantes para ver o resultado.");
        }
        RepoSettingsLabel::PushQueued => {
            text.literal("Um push está na fila; atualize em instantes para ver o resultado.");
        }
        RepoSettingsLabel::StopMirroring => text.literal("Parar de espelhar"),
        RepoSettingsLabel::StopHint => {
            text.literal("O repositório mantém o conteúdo, para de buscar e aceita pushes como qualquer outro.");
        }
        RepoSettingsLabel::PushMirrors => text.literal("Espelhos de push"),
        RepoSettingsLabel::PushMirrorsHint => {
            text.literal("Remotos que recebem os branches e tags deste repositório após cada push.");
        }
        RepoSettingsLabel::PushMirrorsLead => {
            text.literal(
                "Remotos que recebem os branches e tags deste repositório. Cada push sobrescreve os branches e tags \
                 do remoto e exclui os que não existem mais aqui.",
            );
        }
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("Os espelhos de push estão desativados"),
        RepoSettingsLabel::PushMirrorsDisabledText => {
            text.literal("Um administrador desta instância desativou os espelhos de push.");
        }
        RepoSettingsLabel::NoPushMirrors => text.literal("Ainda não há espelhos de push"),
        RepoSettingsLabel::NoPushMirrorsText => {
            text.literal("Adicione um remoto abaixo para manter uma cópia deste repositório em outro lugar.");
        }
        RepoSettingsLabel::Paused => text.literal("Pausado"),
        RepoSettingsLabel::OnPush => text.literal("A cada push"),
        RepoSettingsLabel::LastPush => text.literal("Último push"),
        RepoSettingsLabel::NotPushedYet => text.literal("Ainda sem push"),
        RepoSettingsLabel::As => text.literal("como"),
        RepoSettingsLabel::SecretConfigured => text.literal("segredo configurado"),
        RepoSettingsLabel::PushNow => text.literal("Fazer push agora"),
        RepoSettingsLabel::Edit => text.literal("Editar"),
        RepoSettingsLabel::Remove => text.literal("Remover"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("Adicionar um espelho de push"),
        RepoSettingsLabel::AddPushMirror => text.literal("Adicionar espelho de push"),
        RepoSettingsLabel::PushMirrorAdded => {
            text.literal(
                "Espelho de push adicionado. O primeiro push está a caminho; atualize em instantes para ver o \
                 resultado.",
            );
        }
        RepoSettingsLabel::PushMirrorRemoved => {
            text.literal("Espelho de push removido. O remoto mantém o que já tem.");
        }
        RepoSettingsLabel::EditPushMirror => text.literal("Editar espelho de push"),
        RepoSettingsLabel::PushMirror => text.literal("Espelho de push"),
        RepoSettingsLabel::Enabled => text.literal("Ativado"),
        RepoSettingsLabel::EnabledHint => {
            text.literal("Um espelho pausado mantém as configurações e não recebe push.");
        }
        RepoSettingsLabel::Cancel => text.literal("Cancelar"),
        RepoSettingsLabel::RemoteUrl => text.literal("URL do remoto"),
        RepoSettingsLabel::UrlHintStart => text.literal("Uma URL"),
        RepoSettingsLabel::UrlHintOr => text.literal("ou"),
        RepoSettingsLabel::UrlHintEnd => text.literal("sem nome de usuário nem senha."),
        RepoSettingsLabel::UserName => text.literal("Nome de usuário"),
        RepoSettingsLabel::PasswordOrToken => text.literal("Senha ou token"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("Configurado — deixe em branco para manter"),
        RepoSettingsLabel::SecretHint => {
            text.literal("Enviado ao remoto por autenticação HTTP Basic. Não pode ser exibido novamente.");
        }
        RepoSettingsLabel::SecretHintKept => {
            text.literal(
                "Enviado ao remoto por autenticação HTTP Basic. Não pode ser exibido novamente e precisa ser digitado \
                 de novo se o remoto mudar para outro host.",
            );
        }
        RepoSettingsLabel::PushAfterPush => text.literal("Fazer push após cada push"),
        RepoSettingsLabel::PushAfterPushHint => {
            text.literal(
                "Alguns segundos depois que este repositório muda. Todo espelho também recebe push periodicamente.",
            );
        }
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("Pelo menos ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("Todo espelho recebe push pelo menos a cada ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.literal("Excluir ");
            text.argument(title);
        }
    }
}
