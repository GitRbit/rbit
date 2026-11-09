//! Spanish (`es`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};
use crate::plural::Plural;

entries! { Es:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("Configuración"),
        RepoSettingsLabel::AdminsOf => text.literal("Solo los administradores de"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("pueden ver esta página."),
        RepoSettingsLabel::SettingsSaved => text.literal("Configuración guardada."),
        RepoSettingsLabel::General => text.literal("General"),
        RepoSettingsLabel::Description => text.literal("Descripción"),
        RepoSettingsLabel::Optional => text.literal("opcional"),
        RepoSettingsLabel::Visibility => text.literal("Visibilidad"),
        RepoSettingsLabel::Public => text.literal("Público"),
        RepoSettingsLabel::PublicHint => text.literal("Cualquiera puede ver y clonar este repositorio."),
        RepoSettingsLabel::Private => text.literal("Privado"),
        RepoSettingsLabel::PrivateHint => text.literal("Solo las personas con acceso pueden verlo y clonarlo."),
        RepoSettingsLabel::DefaultBranch => text.literal("Rama predeterminada"),
        RepoSettingsLabel::PushABranch => text.literal("Envía una rama para elegir la rama predeterminada."),
        RepoSettingsLabel::DefaultBranchHint => {
            text.literal("Se muestra primero al explorar y es la que extrae git clone.");
        }
        RepoSettingsLabel::SaveChanges => text.literal("Guardar cambios"),
        RepoSettingsLabel::RepositoryName => text.literal("Nombre del repositorio"),
        RepoSettingsLabel::Name => text.literal("Nombre"),
        RepoSettingsLabel::RenameHint => {
            text.literal("Los enlaces y las URL de clonación que usan el nombre anterior dejan de funcionar.");
        }
        RepoSettingsLabel::Rename => text.literal("Cambiar el nombre"),
        RepoSettingsLabel::DangerZone => text.literal("Zona de peligro"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("Desarchivar este repositorio"),
        RepoSettingsLabel::UnarchiveHint => text.literal(
            "Vuelve a aceptar envíos, incidencias, solicitudes de cambios, comentarios y versiones, y sus réplicas se \
             reanudan.",
        ),
        RepoSettingsLabel::Unarchive => text.literal("Desarchivar repositorio"),
        RepoSettingsLabel::ArchiveTitle => text.literal("Archivar este repositorio"),
        RepoSettingsLabel::ArchiveHint => text.literal(
            "Pasa a ser de solo lectura: se puede seguir clonando, bifurcando y marcando con estrella, pero nadie \
             puede enviar, fusionar ni cambiar sus incidencias, solicitudes de cambios o versiones, y sus réplicas se \
             pausan. Puedes desarchivarlo cuando quieras.",
        ),
        RepoSettingsLabel::Archive => text.literal("Archivar repositorio"),
        RepoSettingsLabel::DeleteTitle => text.literal("Eliminar este repositorio"),
        RepoSettingsLabel::DeleteHint => text.literal(
            "Sus archivos, su historial y sus estrellas se eliminan para siempre. Esto no se puede deshacer.",
        ),
        RepoSettingsLabel::Type => text.literal("Escribe"),
        RepoSettingsLabel::ToConfirm => text.literal("para confirmar"),
        RepoSettingsLabel::DeleteRepository => text.literal("Eliminar repositorio"),
        RepoSettingsLabel::DeployKeys => text.literal("Claves de despliegue"),
        RepoSettingsLabel::DeployKeysHint => text.literal(
            "Claves que dan a un servidor o a un trabajo de CI acceso SSH solo a este repositorio.",
        ),
        RepoSettingsLabel::DeployKeysLeadStart => {
            text.literal("Una clave de despliegue da a un servidor o a un trabajo de CI acceso SSH solo a");
        }
        RepoSettingsLabel::DeployKeysLeadEnd => {
            text.literal(". Las claves son de solo lectura salvo que les permitas enviar.");
        }
        RepoSettingsLabel::NewDeployKey => text.literal("Nueva clave de despliegue"),
        RepoSettingsLabel::Title => text.literal("Título"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("p. ej., Servidor de producción"),
        RepoSettingsLabel::TitleHint => text.literal("Por defecto, el comentario de la clave."),
        RepoSettingsLabel::Key => text.literal("Clave"),
        RepoSettingsLabel::KeyHint => text.literal(
            "Una clave solo puede pertenecer a un repositorio o a una cuenta; genera una clave distinta para cada uno.",
        ),
        RepoSettingsLabel::AllowWrite => text.literal("Permitir acceso de escritura"),
        RepoSettingsLabel::AllowWriteHint => {
            text.literal("La clave puede enviar a este repositorio además de clonarlo.");
        }
        RepoSettingsLabel::AddDeployKey => text.literal("Añadir clave de despliegue"),
        RepoSettingsLabel::Keys => text.literal("Claves"),
        RepoSettingsLabel::NoDeployKeys => text.literal("No hay claves de despliegue"),
        RepoSettingsLabel::NoDeployKeysText => text.literal(
            "Añade una clave para que un servidor o un trabajo de CI pueda clonar este repositorio por SSH.",
        ),
        RepoSettingsLabel::Delete => text.literal("Eliminar"),
        RepoSettingsLabel::Mirror => text.literal("Réplica"),
        RepoSettingsLabel::MirrorHint => {
            text.literal("Origen, intervalo de obtención y resultado de la última obtención.");
        }
        RepoSettingsLabel::MirrorLead => text.literal(
            "Este repositorio obtiene las ramas y los tags de su origen y no acepta envíos.",
        ),
        RepoSettingsLabel::Upstream => text.literal("Origen"),
        RepoSettingsLabel::LastFetch => text.literal("Última obtención"),
        RepoSettingsLabel::Failed => text.literal("Fallida"),
        RepoSettingsLabel::Ok => text.literal("Correcta"),
        RepoSettingsLabel::NotYet => text.literal("Todavía no"),
        RepoSettingsLabel::NextFetch => text.literal("Próxima obtención"),
        RepoSettingsLabel::Error => text.literal("Error"),
        RepoSettingsLabel::FetchNow => text.literal("Obtener ahora"),
        RepoSettingsLabel::FetchEvery => text.literal("Obtener cada"),
        RepoSettingsLabel::Minutes => text.literal("minutos"),
        RepoSettingsLabel::Save => text.literal("Guardar"),
        RepoSettingsLabel::Saved => text.literal("Guardado."),
        RepoSettingsLabel::FetchQueued => text.literal(
            "Hay una obtención en cola; actualiza la página en un momento para ver su resultado.",
        ),
        RepoSettingsLabel::PushQueued => text.literal(
            "Hay un envío en cola; actualiza la página en un momento para ver su resultado.",
        ),
        RepoSettingsLabel::StopMirroring => text.literal("Dejar de replicar"),
        RepoSettingsLabel::StopHint => text.literal(
            "El repositorio conserva su contenido, deja de obtener cambios y acepta envíos como cualquier otro.",
        ),
        RepoSettingsLabel::PushMirrors => text.literal("Réplicas de envío"),
        RepoSettingsLabel::PushMirrorsHint => text.literal(
            "Remotos que reciben las ramas y los tags de este repositorio después de cada envío.",
        ),
        RepoSettingsLabel::PushMirrorsLead => text.literal(
            "Remotos que reciben las ramas y los tags de este repositorio. Cada envío sobrescribe las ramas y los \
             tags del remoto y elimina los que ya no existen aquí.",
        ),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("Las réplicas de envío están desactivadas"),
        RepoSettingsLabel::PushMirrorsDisabledText => {
            text.literal("Un administrador de esta instancia desactivó las réplicas de envío.");
        }
        RepoSettingsLabel::NoPushMirrors => text.literal("Aún no hay réplicas de envío"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal(
            "Añade un remoto abajo para conservar una copia de este repositorio en otro lugar.",
        ),
        RepoSettingsLabel::Paused => text.literal("En pausa"),
        RepoSettingsLabel::OnPush => text.literal("Al enviar"),
        RepoSettingsLabel::LastPush => text.literal("Último envío"),
        RepoSettingsLabel::NotPushedYet => text.literal("Aún sin enviar"),
        RepoSettingsLabel::As => text.literal("como"),
        RepoSettingsLabel::SecretConfigured => text.literal("secreto configurado"),
        RepoSettingsLabel::PushNow => text.literal("Enviar ahora"),
        RepoSettingsLabel::Edit => text.literal("Editar"),
        RepoSettingsLabel::Remove => text.literal("Quitar"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("Añadir una réplica de envío"),
        RepoSettingsLabel::AddPushMirror => text.literal("Añadir réplica de envío"),
        RepoSettingsLabel::PushMirrorAdded => text.literal(
            "Se añadió la réplica de envío. El primer envío está en camino; actualiza la página en un momento para \
             ver su resultado.",
        ),
        RepoSettingsLabel::PushMirrorRemoved => {
            text.literal("Se quitó la réplica de envío. El remoto conserva lo que tiene.");
        }
        RepoSettingsLabel::EditPushMirror => text.literal("Editar réplica de envío"),
        RepoSettingsLabel::PushMirror => text.literal("Réplica de envío"),
        RepoSettingsLabel::Enabled => text.literal("Activada"),
        RepoSettingsLabel::EnabledHint => {
            text.literal("Una réplica en pausa conserva su configuración y no recibe envíos.");
        }
        RepoSettingsLabel::Cancel => text.literal("Cancelar"),
        RepoSettingsLabel::RemoteUrl => text.literal("URL del remoto"),
        RepoSettingsLabel::UrlHintStart => text.literal("Una URL"),
        RepoSettingsLabel::UrlHintOr => text.literal("o"),
        RepoSettingsLabel::UrlHintEnd => text.literal("sin nombre de usuario ni contraseña."),
        RepoSettingsLabel::UserName => text.literal("Nombre de usuario"),
        RepoSettingsLabel::PasswordOrToken => text.literal("Contraseña o token"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("Configurado: déjalo vacío para conservarlo"),
        RepoSettingsLabel::SecretHint => text.literal(
            "Se envía al remoto con autenticación HTTP Basic. No se puede volver a mostrar.",
        ),
        RepoSettingsLabel::SecretHintKept => text.literal(
            "Se envía al remoto con autenticación HTTP Basic. No se puede volver a mostrar y hay que introducirlo de \
             nuevo si el remoto pasa a otro host.",
        ),
        RepoSettingsLabel::PushAfterPush => text.literal("Enviar después de cada envío"),
        RepoSettingsLabel::PushAfterPushHint => text.literal(
            "Unos segundos después de que cambie este repositorio. Además, todas las réplicas se envían de forma \
             programada.",
        ),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("Al menos ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("Todas las réplicas se envían al menos cada ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.literal("Eliminar ");
            text.argument(title);
        }
    }
}
