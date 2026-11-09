//! Spanish (`es`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("Agentes"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal(
                "Los agentes de la instancia toman trabajos de los propietarios que permite un administrador, o de \
                 todos los propietarios con ",
            );
            text.argument(setting);
            text.literal(".");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("Token de registro creado"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal(
                "Cópialo ahora: no se vuelve a mostrar. En la máquina que debe ejecutar los trabajos, ejecuta el \
                 comando de abajo y pega el token cuando lo pida; después inicia el agente con ",
            );
            text.argument(command);
            text.literal(".");
        }
        ActionsRunnersMessage::Token => text.literal("Token"),
        ActionsRunnersMessage::CopyToken => text.literal("Copiar el token"),
        ActionsRunnersMessage::Copy => text.literal("Copiar"),
        ActionsRunnersMessage::Command => text.literal("Comando"),
        ActionsRunnersMessage::CopyCommand => text.literal("Copiar el comando"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("Las etiquetas indican cómo se ejecutan los trabajos: ");
            text.argument(container);
            text.literal(" se ejecuta en Docker o Podman; ");
            text.argument(host);
            text.literal(
                " se ejecuta en la propia máquina sin ningún aislamiento, solo para repositorios de confianza.",
            );
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "este repositorio"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "tus repositorios"),
        ActionsRunnersMessage::InstanceIntro { program } => {
            intro(text, program, None, "todos los propietarios autorizados a usar agentes de la instancia");
        }
        ActionsRunnersMessage::NoRunners => text.literal("Aún no hay agentes"),
        ActionsRunnersMessage::NoRunnersHint => text.literal(
            "Crea un token de registro abajo y registra un agente con él. Los trabajos esperan hasta que haya en \
             línea un agente con sus etiquetas.",
        ),
        ActionsRunnersMessage::Online => text.literal("en línea"),
        ActionsRunnersMessage::Offline => text.literal("sin conexión"),
        ActionsRunnersMessage::Disabled => text.literal("desactivado"),
        ActionsRunnersMessage::NeverSeen => text.literal("nunca visto"),
        ActionsRunnersMessage::OwnerScope => text.literal("propietario"),
        ActionsRunnersMessage::InstanceScope => text.literal("instancia"),
        ActionsRunnersMessage::Ephemeral => text.literal("efímero"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("host: sin aislamiento"),
        ActionsRunnersMessage::HostWarning => text.literal(
            "Los trabajos con sus etiquetas de host se ejecutan como el usuario del agente, sin aislamiento",
        ),
        ActionsRunnersMessage::Capacity { count } => {
            text.number(count);
            text.literal(" a la vez");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("visto ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("Activar"),
        ActionsRunnersMessage::Disable => text.literal("Desactivar"),
        ActionsRunnersMessage::Remove => text.literal("Quitar"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.literal("Quitar ");
            text.argument(name);
        }
        ActionsRunnersMessage::TokensTitle => text.literal("Tokens de registro"),
        ActionsRunnersMessage::TokensIntro => text.literal(
            "Un agente canjea una sola vez un token de registro por su propia credencial. Los tokens son de un solo \
             uso y caducan, salvo que elijas otra cosa.",
        ),
        ActionsRunnersMessage::ExpiresAfter => text.literal("Caduca al cabo de"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("Solo estas etiquetas"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(opcional)"),
        ActionsRunnersMessage::Reusable => text.literal(
            "Permitir que cualquier número de agentes se registre con él hasta que caduque",
        ),
        ActionsRunnersMessage::NewToken => text.literal("Nuevo token de registro"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("Creado ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("caduca ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("ya no se puede usar"),
        ActionsRunnersMessage::Revoked => text.literal("revocado"),
        ActionsRunnersMessage::Expired => text.literal("caducado"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("usos ilimitados"),
        ActionsRunnersMessage::UsedUp => text.literal("agotado"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " uso restante",
                _ => " usos restantes",
            });
        }
        ActionsRunnersMessage::Revoke => text.literal("Revocar"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " hora",
                _ => " horas",
            });
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " día",
                _ => " días",
            });
        }
    }
}

/// "Máquinas que ejecutan rbit-runner y toman los trabajos de este repositorio."
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("Máquinas que ejecutan ");
    text.argument(program);
    text.literal(" y toman los trabajos de ");
    match owner {
        Some(owner) => text.argument(owner),
        None => text.literal(subject),
    }
    text.literal(".");
}
