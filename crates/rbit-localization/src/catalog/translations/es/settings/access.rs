//! Spanish (`es`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Es:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("Configuración"),
        AccessMessage::Collaborators => text.literal("Colaboradores"),
        AccessMessage::IntroLead => text.literal("Las personas que añadas aquí obtienen acceso a"),
        AccessMessage::IntroDirectly => text.literal("directamente"),
        AccessMessage::IntroOrganization => text.literal(", pertenezcan o no a la organización"),
        AccessMessage::IntroTail => text.literal("."),
        AccessMessage::AddCollaborator => text.literal("Añadir un colaborador"),
        AccessMessage::Username => text.literal("Nombre de usuario"),
        AccessMessage::Permission => text.literal("Permiso"),
        AccessMessage::Add => text.literal("Añadir"),
        AccessMessage::People => text.literal("Personas"),
        AccessMessage::NoCollaborators => text.literal("No hay colaboradores"),
        AccessMessage::NoCollaboratorsText => text.literal(
            "Añade a alguien por su nombre de usuario para darle acceso a este repositorio.",
        ),
        AccessMessage::You => text.literal("(tú)"),
        AccessMessage::Save => text.literal("Guardar"),
        AccessMessage::Leave => text.literal("Abandonar"),
        AccessMessage::Remove => text.literal("Quitar"),
        AccessMessage::Teams => text.literal("Equipos"),
        AccessMessage::ManageTeams => text.literal("Gestionar equipos"),
        AccessMessage::NoTeams => text.literal("No hay equipos"),
        AccessMessage::NoTeamsText => text.literal(
            "Da acceso a un equipo desde la página del equipo; sus miembros obtienen entonces ese permiso aquí.",
        ),
        AccessMessage::Transfer => text.literal("Transferir la propiedad"),
        AccessMessage::TransferLead => text.literal("Mueve"),
        AccessMessage::TransferTail => text.literal(
            "a otra cuenta. Su historial, sus colaboradores y sus estrellas se mueven con él; los enlaces y las URL \
             de clonación que usan la dirección anterior dejan de funcionar, y los equipos de la organización actual \
             pierden su acceso.",
        ),
        AccessMessage::NewOwner => text.literal("Nuevo propietario"),
        AccessMessage::AdminEntry => text.literal(
            "Como administrador del sitio, puedes indicar cualquier usuario u organización.",
        ),
        AccessMessage::Organization => text.literal("Organización"),
        AccessMessage::OwnerHint => text.literal("Tu cuenta o una organización a la que perteneces."),
        AccessMessage::TypeLead => text.literal("Escribe"),
        AccessMessage::TypeTail => text.literal("para confirmar"),
        AccessMessage::TransferRepository => text.literal("Transferir repositorio"),
        AccessMessage::Nowhere => text.literal("No hay adónde transferirlo"),
        AccessMessage::NowhereText => text.literal(
            "No perteneces a ninguna organización en la que puedas crear repositorios, y este repositorio ya es tuyo.",
        ),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.literal("Permiso de ");
            text.argument(name);
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("Guardar el permiso de ");
            text.argument(name);
        }
        AccessText::RemoveLabel(name) => {
            text.literal("Quitar a ");
            text.argument(name);
        }
        AccessText::Added(date) => {
            text.literal("Añadido ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("Colaboradores · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("Transferir · ");
            text.argument(repository);
        }
    }
}
