//! Spanish (`es`) entries for `catalog/repo/browse.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};
use crate::plural::Plural;

entries! { Es:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("Acerca de"),
        RepoBrowseLabel::NoDescription => text.literal("Sin descripción."),
        RepoBrowseLabel::Visibility => text.literal("Visibilidad"),
        RepoBrowseLabel::History => text.literal("Historial"),
        RepoBrowseLabel::CommitHistory => text.literal("Historial de commits"),
        RepoBrowseLabel::AddFile => text.literal("Añadir archivo"),
        RepoBrowseLabel::UploadFiles => text.literal("Subir archivos"),
        RepoBrowseLabel::Files => text.literal("Archivos"),
        RepoBrowseLabel::ParentDirectory => text.literal("Carpeta superior"),
        RepoBrowseLabel::Path => text.literal("Ruta"),
        RepoBrowseLabel::Latest => text.literal("Más reciente"),
        RepoBrowseLabel::YourAccess => text.literal("Tu acceso"),
        RepoBrowseLabel::LeaveRepository => text.literal("Abandonar el repositorio"),
        RepoBrowseLabel::Languages => text.literal("Lenguajes"),
        RepoBrowseLabel::RepositoryLinks => text.literal("Enlaces del repositorio"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Almacenado con Git LFS"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("El objeto no está en este servidor"),
        RepoBrowseLabel::EditThisFile => text.literal("Editar este archivo"),
        RepoBrowseLabel::DeleteThisFile => text.literal("Eliminar este archivo"),
        RepoBrowseLabel::Blame => text.literal("Autoría"),
        RepoBrowseLabel::Raw => text.literal("Sin formato"),
        RepoBrowseLabel::Download => text.literal("Descargar"),
        RepoBrowseLabel::BinaryNotShown => text.literal("No se muestra el archivo binario."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("Este archivo es demasiado grande para mostrarlo."),
        RepoBrowseLabel::ViewRaw => text.literal("Ver sin formato"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("Documentos del repositorio"),
        RepoBrowseLabel::DocumentOutline => text.literal("Esquema del documento"),
        RepoBrowseLabel::FilterHeadings => text.literal("Filtrar encabezados"),
        RepoBrowseLabel::DocumentHeadings => text.literal("Encabezados del documento"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("No hay encabezados coincidentes."),
        RepoBrowseLabel::EmptyTitle => text.literal("Este repositorio está vacío"),
        RepoBrowseLabel::EmptyLead => {
            text.literal("Envía un proyecto existente o empieza uno nuevo desde la línea de comandos.");
        }
        RepoBrowseLabel::RepositoryUrl => text.literal("URL del repositorio"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("Copiar la URL del repositorio"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("URL SSH del repositorio"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("Copiar la URL SSH del repositorio"),
        RepoBrowseLabel::EmptySshNote => text.literal(
            "Los comandos siguientes usan HTTPS. Para enviar por SSH, usa la URL SSH y añade una clave en ",
        ),
        RepoBrowseLabel::CreateNewRepository => text.literal("Crear un repositorio nuevo"),
        RepoBrowseLabel::PushExistingRepository => text.literal("Enviar un repositorio existente"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("Cerrar el explorador de archivos"),
        RepoBrowseLabel::LanguageFiles => text.literal("Archivos del lenguaje"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("No se encontraron archivos de este lenguaje."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("No hay archivos coincidentes."),
        RepoBrowseLabel::FileContent => text.literal("Contenido del archivo"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("Selecciona un archivo para ver su contenido."),
        RepoBrowseLabel::SearchThisListing => text.literal("Buscar en esta lista"),
        RepoBrowseLabel::SearchNames => text.literal("Buscar nombres"),
        RepoBrowseLabel::Search => text.literal("Buscar"),
        RepoBrowseLabel::ListingPages => text.literal("Páginas de la lista"),
        RepoBrowseLabel::Previous => text.literal("Anterior"),
        RepoBrowseLabel::Next => text.literal("Siguiente"),
        RepoBrowseLabel::BranchesAndTags => text.literal("Ramas y tags"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("Explorar ramas y tags"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("Buscar una rama o un tag"),
        RepoBrowseLabel::Find => text.literal("Buscar"),
        RepoBrowseLabel::Branch => text.literal("Rama"),
        RepoBrowseLabel::Tag => text.literal("Tag"),
        RepoBrowseLabel::Default => text.literal("predeterminada"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("No hay ramas ni tags coincidentes."),
        RepoBrowseLabel::NoForksYet => text.literal("Aún no hay bifurcaciones"),
        RepoBrowseLabel::ForksEmpty => text.literal(
            "Las bifurcaciones son copias de este repositorio en las que otras personas trabajan por su cuenta.",
        ),
        RepoBrowseLabel::ForkLead => text.literal(
            "Una bifurcación es tu propia copia de las ramas y los tags del repositorio. Conserva la visibilidad \
             del original.",
        ),
        RepoBrowseLabel::ForkLeadPrivate => text.literal(
            "Una bifurcación es tu propia copia de las ramas y los tags del repositorio. Conserva la visibilidad \
             del original: esta seguirá siendo privada.",
        ),
        RepoBrowseLabel::Owner => text.literal("Propietario"),
        RepoBrowseLabel::Name => text.literal("Nombre"),
        RepoBrowseLabel::CreateFork => text.literal("Crear bifurcación"),
        RepoBrowseLabel::Cancel => text.literal("Cancelar"),
        RepoBrowseLabel::NoStargazersYet => text.literal("Aún nadie lo ha marcado con estrella"),
        RepoBrowseLabel::StargazersEmpty => {
            text.literal("Aquí aparecen las personas que marcan este repositorio con estrella.");
        }
        RepoBrowseLabel::RepositoryActivity => text.literal("Actividad del repositorio"),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "rama",
            _ => "ramas",
        }),
        RepoBrowseMessage::TagNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "tag",
            _ => "tags",
        }),
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("Acceso de ");
            text.argument(permission);
            text.literal(" como colaborador.");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " línea",
                _ => " líneas",
            });
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.literal("Archivos de ");
            text.argument(language);
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " resultado · ",
                _ => " resultados · ",
            });
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · ");
            text.number(total as u64);
            text.literal(match text.plural(total as u64) {
                Plural::One => " referencia",
                _ => " referencias",
            });
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.literal("Bifurcar ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
    }
}
