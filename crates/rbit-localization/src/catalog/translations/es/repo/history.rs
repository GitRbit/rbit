//! Spanish (`es`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};
use crate::plural::Plural;

entries! { Es:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("Explorar los archivos en este commit"),
        RepoHistoryLabel::BrowseFiles => text.literal("Explorar archivos"),
        RepoHistoryLabel::Pagination => text.literal("Paginación"),
        RepoHistoryLabel::Newer => text.literal("Más recientes"),
        RepoHistoryLabel::Older => text.literal("Más antiguos"),
        RepoHistoryLabel::Committed => text.literal("hizo commit"),
        RepoHistoryLabel::CommittedBy => text.literal("commit hecho por"),
        RepoHistoryLabel::Commit => text.literal("commit"),
        RepoHistoryLabel::Checks => text.literal("Comprobaciones"),
        RepoHistoryLabel::ViewFile => text.literal("Ver archivo"),
        RepoHistoryLabel::PathInThatCommit => text.literal("Ruta en ese commit"),
        RepoHistoryLabel::FileEmpty => text.literal("Este archivo está vacío."),
        RepoHistoryLabel::BinaryCannotBeBlamed => {
            text.literal("No se puede ver la autoría de los archivos binarios.");
        }
        RepoHistoryLabel::TooLargeToBlame => {
            text.literal("Este archivo es demasiado grande para ver su autoría.");
        }
        RepoHistoryLabel::ViewTheFile => text.literal("Ver el archivo"),
        RepoHistoryLabel::CompareChanges => text.literal("Comparar cambios"),
        RepoHistoryLabel::CompareLead => text.literal(
            "Elige una base y la rama, el tag o el commit que quieres comparar con ella, también entre \
             bifurcaciones.",
        ),
        RepoHistoryLabel::Comparison => text.literal("Comparación"),
        RepoHistoryLabel::MergeBase => text.literal("base de fusión"),
        RepoHistoryLabel::Swap => text.literal("Intercambiar"),
        RepoHistoryLabel::CreatePullRequest => text.literal("Crear solicitud de cambios"),
        RepoHistoryLabel::UnrelatedHistories => text.literal(
            "Estas revisiones tienen historiales completamente distintos, así que no hay diferencias que mostrar.",
        ),
        RepoHistoryLabel::NothingToCompare => text.literal("No hay nada que comparar"),
        RepoHistoryLabel::NothingToCompareText => {
            text.literal("Todo lo que contiene la revisión comparada ya está en la base.");
        }
        RepoHistoryLabel::Commits => text.literal("Commits"),
        RepoHistoryLabel::Compare => text.literal("Comparar"),
        RepoHistoryLabel::CompareWithDefault => text.literal("Comparar con la rama predeterminada"),
        RepoHistoryLabel::BranchView => text.literal("Vista de ramas"),
        RepoHistoryLabel::List => text.literal("Lista"),
        RepoHistoryLabel::GitTree => text.literal("Árbol de Git"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("No hay referencias coincidentes."),
        RepoHistoryLabel::NoBranchesYet => text.literal("Aún no hay ramas"),
        RepoHistoryLabel::NoBranchesText => text.literal("Envía un commit para crear la primera rama."),
        RepoHistoryLabel::NoTagsYet => text.literal("Aún no hay tags"),
        RepoHistoryLabel::NoTagsText => {
            text.literal("Los tags marcan versiones y otros puntos importantes del historial.");
        }
        RepoHistoryLabel::Protected => text.literal("Protegida"),
        RepoHistoryLabel::ProtectedTitle => text.literal("Una regla de protección de ramas se aplica a esta rama"),
        RepoHistoryLabel::DownloadTarGz => text.literal("Descargar tar.gz"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("Gráfico de commits de las ramas"),
        RepoHistoryLabel::GraphHelp => text.literal(
            "Sigue las líneas de colores para recorrer el historial. Selecciona una pista para resaltar sus commits.",
        ),
        RepoHistoryLabel::ClearHighlight => text.literal("Quitar el resaltado"),
        RepoHistoryLabel::HistoryConnections => text.literal("Conexiones del historial"),
        RepoHistoryLabel::CommitDetails => text.literal("Detalles del commit"),
        RepoHistoryLabel::Merge => text.literal("Fusión"),
        RepoHistoryLabel::GraphTruncated => text.literal(
            "Se muestran los 200 commits más recientes de todas las ramas. Las líneas de abajo continúan en el \
             historial anterior. Abre el historial de una rama para explorar más.",
        ),
        RepoHistoryLabel::LinesAdded => text.literal("Añadidas"),
        RepoHistoryLabel::LinesDeleted => text.literal("Eliminadas"),
        RepoHistoryLabel::StatusAdded => text.literal("Añadido"),
        RepoHistoryLabel::StatusDeleted => text.literal("Eliminado"),
        RepoHistoryLabel::StatusModified => text.literal("Modificado"),
        RepoHistoryLabel::StatusRenamed => text.literal("Renombrado"),
        RepoHistoryLabel::StatusCopied => text.literal("Copiado"),
        RepoHistoryLabel::ChangedFiles => text.literal("Archivos modificados"),
        RepoHistoryLabel::FileTree => text.literal("Árbol de archivos"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("Filtrar los archivos modificados"),
        RepoHistoryLabel::FilterFiles => text.literal("Filtrar archivos…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("Ningún archivo coincide."),
        RepoHistoryLabel::Changes => text.literal("Cambios"),
        RepoHistoryLabel::ToggleFileTree => text.literal("Mostrar u ocultar el árbol de archivos"),
        RepoHistoryLabel::CollapseAll => text.literal("Contraer todo"),
        RepoHistoryLabel::ExpandAll => text.literal("Expandir todo"),
        RepoHistoryLabel::DiffTruncated => text.literal(
            "Este cambio es muy grande, así que no se muestran algunas diferencias. Clona el repositorio para ver el \
             cambio completo.",
        ),
        RepoHistoryLabel::CollapseHint => {
            text.literal("Haz clic para contraer o expandir · Alt+clic para todos los archivos");
        }
        RepoHistoryLabel::CopyPath => text.literal("Copiar ruta"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("Ver el archivo en este commit"),
        RepoHistoryLabel::View => text.literal("Ver"),
        RepoHistoryLabel::BinaryNotShown => text.literal("No se muestra el archivo binario."),
        RepoHistoryLabel::DiffTooLarge => text.literal("Las diferencias son demasiado grandes para mostrarlas."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("Commit del submódulo"),
        RepoHistoryLabel::NoContentChanges => text.literal("Sin cambios en el contenido."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.literal("Commits del ");
            text.argument(date);
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("Ver el commit ");
            text.argument(id);
        }
        RepoHistoryMessage::ParentNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "padre",
            _ => "padres",
        }),
        RepoHistoryMessage::BlameOf { file } => {
            text.literal("Autoría de ");
            text.argument(file);
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("Autoría de ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" por delante");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" por detrás");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            let one = text.plural(count as u64) == Plural::One;
            text.literal(if one { "Solo se muestra el " } else { "Solo se muestran los " });
            text.number(count as u64);
            text.literal(if one { " commit más reciente." } else { " commits más recientes." });
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("Resaltar la pista ");
            text.number(track as u64);
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("Pista ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("Rama: ");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " rama más",
                _ => " ramas más",
            });
        }
        RepoHistoryMessage::FilesChangedNoun { count } => text.literal(match text.plural(count as u64) {
            Plural::One => "archivo modificado",
            _ => "archivos modificados",
        }),
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("Comparar ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
