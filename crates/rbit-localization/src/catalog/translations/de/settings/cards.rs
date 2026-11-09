//! German (`de`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { De:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => {
            text.literal(
                "Workflows bei Pushes und Pull-Requests ausführen, mit den Secrets und Variablen, die sie verwenden.",
            );
        }
        CardMessage::OrgActions => {
            text.literal("Secrets und Variablen für die Workflows aller Repositorys dieser Organisation.");
        }
        CardMessage::Collaborators => text.literal("Mitarbeitende"),
        CardMessage::CollaboratorsText => {
            text.literal("Personen und Teams mit Zugriff auf dieses Repository und was sie jeweils dürfen.");
        }
        CardMessage::Mirror => text.literal("Spiegel"),
        CardMessage::MirrorText => {
            text.literal("Upstream, Abrufintervall und das Ergebnis des letzten Abrufs.");
        }
        CardMessage::Protection => text.literal("Branch-Schutz"),
        CardMessage::ProtectionText => {
            text.literal(
                "Wichtige Branches vor Force-Pushes, Löschen oder Pushes ohne Pull-Request schützen.",
            );
        }
        CardMessage::PushMirrors => text.literal("Push-Spiegel"),
        CardMessage::PushMirrorsText => {
            text.literal("Remotes, die nach jedem Push die Branches und Tags dieses Repositorys erhalten.");
        }
        CardMessage::DeployKeys => text.literal("Deploy-Schlüssel"),
        CardMessage::DeployKeysText => {
            text.literal(
                "SSH-Schlüssel, die einem Server oder CI-Job Zugriff nur auf dieses Repository gewähren.",
            );
        }
        CardMessage::Teams => text.literal("Teams"),
        CardMessage::TeamsText => {
            text.literal("Mitglieder gruppieren und jeder Gruppe eigenen Zugriff auf Repositorys geben.");
        }
        CardMessage::Transfer => text.literal("Inhaberschaft übertragen"),
        CardMessage::TransferText => {
            text.literal(
                "Dieses Repository an einen anderen Benutzer oder eine andere Organisation übertragen. Seine URL \
                 ändert sich.",
            );
        }
        CardMessage::Webhooks => text.literal("Webhooks"),
        CardMessage::RepoWebhooks => {
            text.literal(
                "Andere Dienste benachrichtigen, wenn Code gepusht, Releases veröffentlicht werden und mehr.",
            );
        }
        CardMessage::OrgWebhooks => {
            text.literal(
                "Andere Dienste über Ereignisse in allen Repositorys dieser Organisation benachrichtigen.",
            );
        }
    }
}
