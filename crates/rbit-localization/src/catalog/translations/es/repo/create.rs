//! Spanish (`es`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};
use crate::plural::Plural;

entries! { Es:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("Nuevo repositorio"),
        RepoCreateLabel::RepositoryType => text.literal("Tipo de repositorio"),
        RepoCreateLabel::Empty => text.literal("Vacío"),
        RepoCreateLabel::Template => text.literal("Plantilla"),
        RepoCreateLabel::TemplateLead => text.literal("Empieza con los archivos de la rama predeterminada de un repositorio en un solo commit inicial."),
        RepoCreateLabel::TemplateRepository => text.literal("Repositorio de plantilla"),
        RepoCreateLabel::TemplatePath => text.literal("propietario/nombre"),
        RepoCreateLabel::UseAsTemplate => text.literal("Usar como plantilla"),
        RepoCreateLabel::Mirror => text.literal("Réplica"),
        RepoCreateLabel::Import => text.literal("Importar"),
        RepoCreateLabel::NewLead => text.literal(
            "Crea un repositorio vacío, importa un proyecto o mantén una réplica sincronizada con su origen.",
        ),
        RepoCreateLabel::Owner => text.literal("Propietario"),
        RepoCreateLabel::Name => text.literal("Nombre"),
        RepoCreateLabel::Description => text.literal("Descripción"),
        RepoCreateLabel::Optional => text.literal("opcional"),
        RepoCreateLabel::Visibility => text.literal("Visibilidad"),
        RepoCreateLabel::Public => text.literal("Público"),
        RepoCreateLabel::Private => text.literal("Privado"),
        RepoCreateLabel::PublicRepository => text.literal("Cualquiera puede ver y clonar este repositorio."),
        RepoCreateLabel::PublicMirror => text.literal("Cualquiera puede ver y clonar la réplica."),
        RepoCreateLabel::PublicImport => text.literal("Cualquiera puede ver y clonar el repositorio."),
        RepoCreateLabel::PrivateHint => text.literal(
            "Solo el propietario puede verlo. En una organización, eso significa sus miembros.",
        ),
        RepoCreateLabel::Cancel => text.literal("Cancelar"),
        RepoCreateLabel::CreateRepository => text.literal("Crear repositorio"),
        RepoCreateLabel::MirrorLead => text.literal(
            "Una réplica obtiene periódicamente las ramas y los tags de un repositorio público alojado en otro \
             lugar. Aquí es de solo lectura: los cambios vienen del origen.",
        ),
        RepoCreateLabel::MirrorsDisabled => text.literal("Las réplicas están desactivadas"),
        RepoCreateLabel::MirrorsDisabledText => {
            text.literal("Un administrador de esta instancia desactivó las réplicas de extracción.");
        }
        RepoCreateLabel::UpstreamUrl => text.literal("URL de origen"),
        RepoCreateLabel::UrlHintStart => text.literal("Una URL"),
        RepoCreateLabel::UrlHintOr => text.literal("o"),
        RepoCreateLabel::UrlHintEnd => text.literal("de un repositorio público."),
        RepoCreateLabel::FromTheUrl => text.literal("a partir de la URL"),
        RepoCreateLabel::FetchEvery => text.literal("Obtener cada"),
        RepoCreateLabel::Minutes => text.literal("minutos"),
        RepoCreateLabel::CreateMirror => text.literal("Crear réplica"),
        RepoCreateLabel::MirrorARepository => text.literal("Replicar un repositorio"),
        RepoCreateLabel::ImportLead => text.literal(
            "Copia un repositorio de GitHub o Gitea con su historial y, si quieres, sus incidencias, solicitudes de \
             cambios, etiquetas, hitos, versiones y archivos de Git LFS. Los números de las incidencias y las \
             solicitudes de cambios se mantienen.",
        ),
        RepoCreateLabel::ImportingDisabled => text.literal("La importación está desactivada"),
        RepoCreateLabel::ImportingDisabledText => {
            text.literal("Un administrador de esta instancia desactivó la importación.");
        }
        RepoCreateLabel::ImportFrom => text.literal("Importar desde"),
        RepoCreateLabel::ServerAddress => text.literal("Dirección del servidor"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("no hace falta para github.com"),
        RepoCreateLabel::Repository => text.literal("Repositorio"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("propietario/nombre o su URL"),
        RepoCreateLabel::AccessToken => text.literal("Token de acceso"),
        RepoCreateLabel::OptionalForPublic => text.literal("opcional para repositorios públicos"),
        RepoCreateLabel::TokenHint => text.literal(
            "Un token que pueda leer el repositorio amplía los límites de solicitudes del origen y es necesario para \
             los repositorios privados. Solo se usa mientras dura la importación y nunca se guarda.",
        ),
        RepoCreateLabel::AsOnTheSource => text.literal("como en el origen"),
        RepoCreateLabel::BringAlong => text.literal("Traer también"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("No compatible: esta instancia no tiene wikis."),
        RepoCreateLabel::StartImport => text.literal("Iniciar importación"),
        RepoCreateLabel::ImportARepository => text.literal("Importar un repositorio"),
        RepoCreateLabel::ImportTitle => text.literal("Importación"),
        RepoCreateLabel::FailedNote => {
            text.literal("Todo lo importado hasta ese momento está en el repositorio.");
        }
        RepoCreateLabel::NotReached => text.literal("No alcanzado"),
        RepoCreateLabel::Started => text.literal("Iniciada"),
        RepoCreateLabel::WaitingForSlot => text.literal("Esperando un hueco libre"),
        RepoCreateLabel::Ended => text.literal("Finalizada"),
        RepoCreateLabel::UpdatesByItself => text.literal("Esta página se actualiza sola."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("Omitido o incompleto"),
        RepoCreateLabel::GoToRepository => text.literal("Ir al repositorio"),
        RepoCreateLabel::OpensAutomatically => text.literal(
            "El repositorio se abrirá automáticamente cuando termine la primera obtención de la réplica.",
        ),
        RepoCreateLabel::FetchInProgress => text.literal("Obtención de la réplica en curso"),
        RepoCreateLabel::TransferSizeNote => text.literal(
            "El tamaño de la transferencia lo determina el origen. Esta página muestra la etapa actual a medida que \
             avanza el trabajo.",
        ),
        RepoCreateLabel::RefreshStatus => text.literal("Actualizar estado"),
        RepoCreateLabel::SettingsAndRetry => text.literal("Configuración de la réplica y reintento"),
        RepoCreateLabel::NoScriptHint => text.literal(
            "Usa Actualizar estado para comprobar el progreso y abre el repositorio cuando esté listo.",
        ),
        RepoCreateLabel::Connecting => text.literal("Conectando con el repositorio de origen."),
        RepoCreateLabel::Fetching => text.literal(
            "Descargando ramas, tags y objetos de Git. Los repositorios grandes pueden tardar varios minutos.",
        ),
        RepoCreateLabel::Finalizing => {
            text.literal("Preparando el contenido del repositorio y la rama predeterminada.");
        }
        RepoCreateLabel::FetchFailedRetry => text.literal(
            "No se pudo obtener la réplica. Un administrador del repositorio puede reintentarlo desde la \
             configuración de la réplica.",
        ),
        RepoCreateLabel::Complete => text.literal("Replicación completada. Abriendo el repositorio…"),
        RepoCreateLabel::Queued => text.literal("Esperando un proceso de réplica disponible."),
        RepoCreateLabel::Stopped => text.literal("La replicación se detuvo. Abriendo el repositorio…"),
        RepoCreateLabel::Mirroring => text.literal("Replicando"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("Al menos ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minuto.",
                _ => " minutos.",
            });
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("Desde ");
            text.argument(source);
            text.literal(":");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("y ");
            text.number(count);
            text.literal(" más (consulta el registro del servidor)");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.literal("Replicando ");
            text.argument(name);
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("No se pudo obtener la réplica: ");
            text.argument(error);
        }
    }
}
