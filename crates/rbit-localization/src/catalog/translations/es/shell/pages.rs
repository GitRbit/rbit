//! Spanish (`es`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};
use crate::plural::Plural;

entries! { Es:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "Página no encontrada",
            Failure::Forbidden => "Acceso denegado",
            Failure::BadRequest => "Solicitud incorrecta",
            Failure::SlowDown => "Más despacio",
            Failure::Unavailable => "No disponible temporalmente",
            Failure::Internal => "Algo salió mal",
        }),
        PageMessage::NotFoundText => {
            text.literal("La página que buscabas no existe o no tienes acceso a ella.");
        }
        PageMessage::UnavailableText => {
            text.literal("El servicio no está disponible temporalmente. Vuelve a intentarlo en breve.");
        }
        PageMessage::InternalText => text.literal("Se produjo un error inesperado. Quedó registrado."),
        PageMessage::GoHome => text.literal("Ir al inicio"),
        PageMessage::Explore => text.literal("Explorar"),
        PageMessage::Offline => text.literal("Estás sin conexión"),
        PageMessage::OfflineText => {
            text.literal("No se pudo contactar con el servidor. Comprueba tu conexión y vuelve a intentarlo.");
        }
        PageMessage::Retry => text.literal("Reintentar"),
        PageMessage::Private => text.literal("Privado"),
        PageMessage::Archived => text.literal("Archivado"),
        PageMessage::Stars(count) => {
            text.number(count.unsigned_abs());
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " estrella",
                _ => " estrellas",
            });
        }
        PageMessage::Pagination => text.literal("Paginación"),
        PageMessage::Previous => text.literal("Anterior"),
        PageMessage::Next => text.literal("Siguiente"),
        PageMessage::PageOf(page, pages) => {
            text.literal("Página ");
            text.number(u64::from(page));
            text.literal(" de ");
            text.number(u64::from(pages));
        }
        PageMessage::EditProfile => text.literal("Editar perfil"),
        PageMessage::Joined(date) => {
            text.literal("Se unió ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("Perfil"),
        PageMessage::Repositories => text.literal("Repositorios"),
        PageMessage::StarsTab => text.literal("Estrellas"),
        PageMessage::NewRepository => text.literal("Nuevo repositorio"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" aún no ha marcado ningún repositorio con estrella.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" aún no tiene repositorios públicos.");
        }
    }
}
