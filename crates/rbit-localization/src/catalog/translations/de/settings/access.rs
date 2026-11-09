//! German (`de`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { De:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("Einstellungen"),
        AccessMessage::Collaborators => text.literal("Mitarbeitende"),
        AccessMessage::IntroLead => text.literal("Personen, die Sie hier hinzufügen, erhalten Zugriff auf "),
        AccessMessage::IntroDirectly => text.literal(" als Mitarbeitende"),
        AccessMessage::IntroOrganization => {
            text.literal(", unabhängig davon, ob sie der Organisation angehören");
        }
        AccessMessage::IntroTail => text.literal("."),
        AccessMessage::AddCollaborator => text.literal("Mitarbeiter hinzufügen"),
        AccessMessage::Username => text.literal("Benutzername"),
        AccessMessage::Permission => text.literal("Berechtigung"),
        AccessMessage::Add => text.literal("Hinzufügen"),
        AccessMessage::People => text.literal("Personen"),
        AccessMessage::NoCollaborators => text.literal("Keine Mitarbeitenden"),
        AccessMessage::NoCollaboratorsText => {
            text.literal("Fügen Sie jemanden per Benutzername hinzu, um Zugriff auf dieses Repository zu gewähren.");
        }
        AccessMessage::You => text.literal("(Sie)"),
        AccessMessage::Save => text.literal("Speichern"),
        AccessMessage::Leave => text.literal("Verlassen"),
        AccessMessage::Remove => text.literal("Entfernen"),
        AccessMessage::Teams => text.literal("Teams"),
        AccessMessage::ManageTeams => text.literal("Teams verwalten"),
        AccessMessage::NoTeams => text.literal("Keine Teams"),
        AccessMessage::NoTeamsText => {
            text.literal(
                "Gewähren Sie einem Team auf dessen Seite Zugriff; seine Mitglieder erhalten dann hier diese Berechtigung.",
            );
        }
        AccessMessage::Transfer => text.literal("Inhaberschaft übertragen"),
        AccessMessage::TransferLead => text.literal("Verschieben Sie"),
        AccessMessage::TransferTail => {
            text.literal(
                "auf ein anderes Konto. Verlauf, Mitarbeitende und Sterne werden mit verschoben; Links und Klon-URLs \
                 mit der alten Adresse funktionieren nicht mehr, und Teams der aktuellen Organisation verlieren ihren \
                 Zugriff.",
            );
        }
        AccessMessage::NewOwner => text.literal("Neuer Inhaber"),
        AccessMessage::AdminEntry => {
            text.literal("Als Website-Administrator können Sie jeden Benutzer und jede Organisation angeben.");
        }
        AccessMessage::Organization => text.literal("Organisation"),
        AccessMessage::OwnerHint => text.literal("Ihr Konto oder eine Organisation, der Sie angehören."),
        AccessMessage::TypeLead => text.literal("Geben Sie"),
        AccessMessage::TypeTail => text.literal("zur Bestätigung ein"),
        AccessMessage::TransferRepository => text.literal("Repository übertragen"),
        AccessMessage::Nowhere => text.literal("Kein mögliches Ziel"),
        AccessMessage::NowhereText => {
            text.literal(
                "Sie gehören keiner Organisation an, in der Sie Repositorys erstellen können, und dieses Repository \
                 gehört bereits Ihnen.",
            );
        }
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.literal("Berechtigung für ");
            text.argument(name);
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("Berechtigung für ");
            text.argument(name);
            text.literal(" speichern");
        }
        AccessText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" entfernen");
        }
        AccessText::Added(date) => {
            text.literal("Hinzugefügt ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("Mitarbeitende · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("Übertragen · ");
            text.argument(repository);
        }
    }
}
