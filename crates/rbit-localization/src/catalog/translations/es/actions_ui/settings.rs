//! Spanish (`es`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Es:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.literal("Los flujos de trabajo de ");
            text.argument(directory);
            text.literal(" se ejecutan en agentes cuando ocurren sus eventos en ");
            text.argument(repository);
            text.literal(".");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("Secretos y variables para los flujos de trabajo de los repositorios de ");
            text.argument(organization);
            text.literal(". Prevalece el secreto o la variable del mismo nombre que tenga el propio repositorio.");
        }
        ActionsSettingsMessage::AccountIntro => text.literal(
            "Secretos y variables para los flujos de trabajo de tus propios repositorios. Prevalece el secreto o la \
             variable del mismo nombre que tenga el propio repositorio.",
        ),
        ActionsSettingsMessage::General => text.literal("General"),
        ActionsSettingsMessage::RunWorkflows => text.literal("Ejecutar flujos de trabajo en este repositorio"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal(
            "Desactivado: no se inicia ninguna ejecución nueva y la pestaña Acciones se oculta. Las ejecuciones \
             anteriores y sus registros se conservan.",
        ),
        ActionsSettingsMessage::DefaultPermission => {
            text.literal("Permiso predeterminado del token de los flujos de trabajo");
        }
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("El ");
            text.argument(key);
            text.literal(
                " de un flujo de trabajo solo puede reducirlo. Las ejecuciones de bifurcaciones y de autores externos \
                 siempre reciben un token de solo lectura.",
            );
        }
        ActionsSettingsMessage::PermissionRead => text.literal("Lectura"),
        ActionsSettingsMessage::PermissionReadHint => text.literal(
            "Los flujos de trabajo pueden leer el contenido, las incidencias y las solicitudes de cambios del \
             repositorio.",
        ),
        ActionsSettingsMessage::PermissionWrite => text.literal("Lectura y escritura"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal(
            "Los flujos de trabajo también pueden enviar y comentar en incidencias y solicitudes de cambios. La \
             protección de ramas sigue aplicándose.",
        ),
        ActionsSettingsMessage::SaveChanges => text.literal("Guardar cambios"),
        ActionsSettingsMessage::RepositoryRunnersHint => text.literal(
            "Los agentes de este repositorio, los que hereda y los tokens de registro.",
        ),
        ActionsSettingsMessage::OrganizationRunnersHint => text.literal(
            "Agentes para todos los repositorios de la organización y tokens de registro.",
        ),
        ActionsSettingsMessage::AccountRunnersHint => {
            text.literal("Agentes para todos los repositorios de tu cuenta y tokens de registro.");
        }
        ActionsSettingsMessage::EnvironmentsHint => text.literal(
            "Entornos de despliegue: revisores obligatorios, ramas permitidas y sus propios secretos y variables.",
        ),
        ActionsSettingsMessage::TrustedTitle => text.literal("Autores de confianza de solicitudes de cambios"),
        ActionsSettingsMessage::TrustedIntro => text.literal(
            "Sus solicitudes de cambios desde bifurcaciones se ejecutan sin esperar aprobación, aunque con un token \
             de solo lectura y sin secretos ni variables. Los autores se añaden desde la aprobación de una ejecución \
             con «ejecutar siempre las solicitudes de cambios de este autor».",
        ),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("No hay autores de confianza"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal(
            "Las solicitudes de cambios de bifurcaciones y de personas sin acceso de escritura esperan a que alguien \
             con acceso de escritura apruebe cada ejecución.",
        ),
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("De confianza ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("De confianza ");
            text.argument(since);
            text.literal(", por ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.literal("Dejar de confiar en ");
            text.argument(name);
        }
        ActionsSettingsMessage::Remove => text.literal("Quitar"),
        ActionsSettingsMessage::Saved => text.literal("Configuración guardada."),
        ActionsSettingsMessage::SecretSaved => text.literal("Secreto guardado."),
        ActionsSettingsMessage::SecretDeleted => text.literal("Secreto eliminado."),
        ActionsSettingsMessage::VariableSaved => text.literal("Variable guardada."),
        ActionsSettingsMessage::VariableDeleted => text.literal("Variable eliminada."),
        ActionsSettingsMessage::Untrusted => {
            text.literal("Las solicitudes de cambios del autor vuelven a necesitar aprobación.");
        }
        ActionsSettingsMessage::RepositoryAccess => text.literal("Acceso a repositorios"),
        ActionsSettingsMessage::AllRepositories => text.literal("Todos los repositorios"),
        ActionsSettingsMessage::AllRepositoriesHint => text.literal(
            "Todos los repositorios de este propietario, incluidos los que se creen más adelante.",
        ),
        ActionsSettingsMessage::SelectedRepositories => text.literal("Repositorios seleccionados"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("Solo los repositorios marcados abajo."),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("Este propietario aún no tiene repositorios."),
    }
}
