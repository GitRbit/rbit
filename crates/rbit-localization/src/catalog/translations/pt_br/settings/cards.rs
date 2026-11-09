//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { PtBr:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => {
            text.literal(
                "Execute fluxos de trabalho em pushes e pull requests, com os segredos e variáveis que eles usam.",
            );
        }
        CardMessage::OrgActions => {
            text.literal("Segredos e variáveis para os fluxos de trabalho de todos os repositórios desta organização.");
        }
        CardMessage::Collaborators => text.literal("Colaboradores"),
        CardMessage::CollaboratorsText => {
            text.literal("Pessoas e equipes com acesso a este repositório e o que cada uma pode fazer.");
        }
        CardMessage::Mirror => text.literal("Espelho"),
        CardMessage::MirrorText => text.literal("Upstream, intervalo de busca e o resultado da última busca."),
        CardMessage::Protection => text.literal("Proteção de branches"),
        CardMessage::ProtectionText => {
            text.literal(
                "Impeça que branches importantes recebam force-push, sejam excluídos ou recebam push sem um pull \
                 request.",
            );
        }
        CardMessage::PushMirrors => text.literal("Espelhos de push"),
        CardMessage::PushMirrorsText => {
            text.literal("Remotos que recebem os branches e tags deste repositório após cada push.");
        }
        CardMessage::DeployKeys => text.literal("Chaves de implantação"),
        CardMessage::DeployKeysText => {
            text.literal("Chaves SSH que dão a um servidor ou job de CI acesso somente a este repositório.");
        }
        CardMessage::Teams => text.literal("Equipes"),
        CardMessage::TeamsText => {
            text.literal("Agrupe membros e dê a cada grupo o próprio acesso aos repositórios.");
        }
        CardMessage::Transfer => text.literal("Transferir propriedade"),
        CardMessage::TransferText => {
            text.literal("Mova este repositório para outro usuário ou organização. A URL dele muda.");
        }
        CardMessage::Webhooks => text.literal("Webhooks"),
        CardMessage::RepoWebhooks => {
            text.literal("Notifique outros serviços quando houver push de código, publicação de releases e mais.");
        }
        CardMessage::OrgWebhooks => {
            text.literal("Notifique outros serviços sobre eventos em todos os repositórios desta organização.");
        }
    }
}
