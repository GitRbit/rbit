//! Spanish (`es`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Es:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("Entornos"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("Un trabajo que nombra un entorno con ");
            text.argument(key);
            text.literal(
                " recibe sus secretos y variables, espera a uno de sus revisores si los tiene y solo se ejecuta desde \
                 las ramas que permite. Un entorno aparece aquí la primera vez que un flujo de trabajo lo nombra.",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("No hay entornos"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("Añade uno abajo o ejecuta un flujo de trabajo cuyo trabajo nombre uno con ");
            text.argument(key);
            text.literal(".");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("Nuevo entorno"),
        ActionsEnvironmentMessage::Name => text.literal("Nombre"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("Añadir entorno"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " revisor",
                _ => " revisores",
            });
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("ramas protegidas"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("ramas ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("Sin protección"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("Reglas de protección"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("Revisores obligatorios"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("un nombre de usuario por línea"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal(
            "Hasta seis personas que puedan leer este repositorio. Un trabajo de este entorno espera hasta que una \
             de ellas lo apruebe; un trabajo rechazado falla. Sin revisores, los trabajos se ejecutan sin revisión.",
        ),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("Impedir la autorrevisión"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => {
            text.literal("Quien inició una ejecución no puede aprobar sus despliegues.");
        }
        ActionsEnvironmentMessage::WaitTimer => text.literal("Temporizador de espera"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal(
            "Minutos que espera un trabajo de este entorno, una vez listo y aprobado, antes de ejecutarse. 0 para no \
             esperar; como máximo 43200 (30 días).",
        ),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("Ramas y tags de despliegue"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("Cualquier rama o tag"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("Solo ramas protegidas"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => {
            text.literal("Ramas que tienen una regla de protección de ramas.");
        }
        ActionsEnvironmentMessage::SelectedBranches => text.literal("Ramas y tags seleccionados"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("Patrones de ramas y tags"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("Un patrón por línea, como en los filtros de los flujos de trabajo. Ramas por nombre (");
            text.argument(branch);
            text.literal("), tags con ");
            text.argument(tags);
            text.literal(" (");
            text.argument(tag);
            text.literal("); ");
            text.argument(exclude);
            text.literal(" excluye.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("Guardar las reglas de protección"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("Eliminar este entorno"),
        ActionsEnvironmentMessage::DeleteHint => text.literal(
            "También se eliminan sus secretos, variables y reglas. Los trabajos que esperan a sus revisores se \
             ejecutan de inmediato.",
        ),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("Eliminar entorno"),
    }
}
