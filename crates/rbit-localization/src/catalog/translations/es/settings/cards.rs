//! Spanish (`es`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { Es:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Acciones"),
        CardMessage::RepoActions => text.literal(
            "Ejecuta flujos de trabajo con los envíos y las solicitudes de cambios, con los secretos y las variables \
             que usan.",
        ),
        CardMessage::OrgActions => text.literal(
            "Secretos y variables para los flujos de trabajo de todos los repositorios de esta organización.",
        ),
        CardMessage::Collaborators => text.literal("Colaboradores"),
        CardMessage::CollaboratorsText => text.literal(
            "Personas y equipos con acceso a este repositorio, y lo que puede hacer cada uno.",
        ),
        CardMessage::Mirror => text.literal("Réplica"),
        CardMessage::MirrorText => text.literal("Origen, intervalo de obtención y resultado de la última obtención."),
        CardMessage::Protection => text.literal("Protección de ramas"),
        CardMessage::ProtectionText => text.literal(
            "Evita que las ramas importantes reciban envíos forzados, se eliminen o reciban envíos sin una solicitud \
             de cambios.",
        ),
        CardMessage::PushMirrors => text.literal("Réplicas de envío"),
        CardMessage::PushMirrorsText => text.literal(
            "Remotos que reciben las ramas y los tags de este repositorio después de cada envío.",
        ),
        CardMessage::DeployKeys => text.literal("Claves de despliegue"),
        CardMessage::DeployKeysText => text.literal(
            "Claves SSH que dan a un servidor o a un trabajo de CI acceso solo a este repositorio.",
        ),
        CardMessage::Teams => text.literal("Equipos"),
        CardMessage::TeamsText => text.literal(
            "Agrupa a los miembros y da a cada grupo su propio acceso a los repositorios.",
        ),
        CardMessage::Transfer => text.literal("Transferir la propiedad"),
        CardMessage::TransferText => text.literal(
            "Mueve este repositorio a otro usuario u organización. Su URL cambia.",
        ),
        CardMessage::Webhooks => text.literal("Webhooks"),
        CardMessage::RepoWebhooks => text.literal(
            "Avisa a otros servicios cuando se envía código, se publican versiones y mucho más.",
        ),
        CardMessage::OrgWebhooks => text.literal(
            "Avisa a otros servicios de los eventos de todos los repositorios de esta organización.",
        ),
    }
}
