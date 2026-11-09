//! French (`fr`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { Fr:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => text.literal(
            "Exécuter des workflows lors des push et des pull requests, avec les secrets et les variables qu’ils utilisent.",
        ),
        CardMessage::OrgActions => text.literal(
            "Secrets et variables des workflows de tous les dépôts de cette organisation.",
        ),
        CardMessage::Collaborators => text.literal("Collaborateurs"),
        CardMessage::CollaboratorsText => text.literal(
            "Les personnes et les équipes qui ont accès à ce dépôt, et ce que chacune peut faire.",
        ),
        CardMessage::Mirror => text.literal("Miroir"),
        CardMessage::MirrorText => text.literal(
            "L’amont, l’intervalle de récupération et le résultat de la dernière récupération.",
        ),
        CardMessage::Protection => text.literal("Protection des branches"),
        CardMessage::ProtectionText => text.literal(
            "Empêcher les push forcés, la suppression des branches importantes ou les push sans pull request.",
        ),
        CardMessage::PushMirrors => text.literal("Miroirs push"),
        CardMessage::PushMirrorsText => text.literal(
            "Des dépôts distants qui reçoivent les branches et les tags de ce dépôt après chaque push.",
        ),
        CardMessage::DeployKeys => text.literal("Clés de déploiement"),
        CardMessage::DeployKeysText => text.literal(
            "Des clés SSH qui donnent à un serveur ou à un job de CI un accès à ce seul dépôt.",
        ),
        CardMessage::Teams => text.literal("Équipes"),
        CardMessage::TeamsText => text.literal(
            "Regrouper les membres et donner à chaque groupe son propre accès aux dépôts.",
        ),
        CardMessage::Transfer => text.literal("Transférer la propriété"),
        CardMessage::TransferText => text.literal(
            "Déplacer ce dépôt vers un autre utilisateur ou une autre organisation. Son URL change.",
        ),
        CardMessage::Webhooks => text.literal("Webhooks"),
        CardMessage::RepoWebhooks => text.literal(
            "Prévenir d’autres services lorsque du code est poussé, qu’une release est publiée, et plus encore.",
        ),
        CardMessage::OrgWebhooks => text.literal(
            "Prévenir d’autres services des événements de tous les dépôts de cette organisation.",
        ),
    }
}
