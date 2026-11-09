//! German (`de`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { De:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("Wählen Sie eine Berechtigung."),
        OrgMessage::Organization => text.literal("Organisation"),
        OrgMessage::Repositories => text.literal("Repositorys"),
        OrgMessage::Members => text.literal("Mitglieder"),
        OrgMessage::Teams => text.literal("Teams"),
        OrgMessage::Settings => text.literal("Einstellungen"),
        OrgMessage::NewOrganization => text.literal("Neue Organisation"),
        OrgMessage::NewIntro => text.literal(
            "Eine Organisation besitzt Repositorys im Namen eines Teams. Sie werden ihr erster Inhaber.",
        ),
        OrgMessage::Name => text.literal("Name"),
        OrgMessage::NameHint => text.literal(
            "Wird in URLs verwendet. Buchstaben, Ziffern, „-“, „_“ und „.“. Benutzer und Organisationen teilen sich einen Namensraum.",
        ),
        OrgMessage::DisplayName => text.literal("Anzeigename"),
        OrgMessage::Optional => text.literal("optional"),
        OrgMessage::Description => text.literal("Beschreibung"),
        OrgMessage::Cancel => text.literal("Abbrechen"),
        OrgMessage::CreateOrganization => text.literal("Organisation erstellen"),
        OrgMessage::New => text.literal("Neu"),
        OrgMessage::NoRepositories => text.literal("Noch keine Repositorys"),
        OrgMessage::NoRepositoriesText => {
            text.literal("Repositorys im Besitz dieser Organisation werden hier aufgeführt.");
        }
        OrgMessage::Profile => text.literal("Profil"),
        OrgMessage::Website => text.literal("Website"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("Profil speichern"),
        OrgMessage::MemberPermission => text.literal("Mitgliederberechtigung"),
        OrgMessage::PermissionLegend => text.literal("Was jedes Mitglied für jedes Repository erhält"),
        OrgMessage::PermissionHint => text.literal(
            "Teams und Mitarbeitende können Rechte nur hinzufügen, nie entziehen. Inhaber verwalten immer jedes Repository.",
        ),
        OrgMessage::SavePermission => text.literal("Berechtigung speichern"),
        OrgMessage::NoPermission => text.literal("Keine Berechtigung"),
        OrgMessage::NoPermissionText => text.literal(
            "Mitglieder sehen nur öffentliche Repositorys und solche, auf die ihre Teams zugreifen können.",
        ),
        OrgMessage::Read => text.literal("Lesen"),
        OrgMessage::ReadText => text.literal("Mitglieder können jedes Repository durchsuchen und klonen."),
        OrgMessage::Write => text.literal("Schreiben"),
        OrgMessage::WriteText => {
            text.literal("Mitglieder können jedes Repository durchsuchen, klonen und dorthin pushen.");
        }
        OrgMessage::DangerZone => text.literal("Gefahrenbereich"),
        OrgMessage::DeleteOrganization => text.literal("Diese Organisation löschen"),
        OrgMessage::DeleteText => text.literal(
            "Dadurch werden die Organisation und alle ihre Mitgliedschaften endgültig entfernt. Dies kann nicht rückgängig gemacht werden.",
        ),
        OrgMessage::TypeToConfirmLead => text.literal("Geben Sie"),
        OrgMessage::TypeToConfirmTail => text.literal("zur Bestätigung ein"),
        OrgMessage::DeleteOrganizationButton => text.literal("Organisation löschen"),
        OrgMessage::AddMember => text.literal("Mitglied hinzufügen"),
        OrgMessage::Username => text.literal("Benutzername"),
        OrgMessage::Role => text.literal("Rolle"),
        OrgMessage::Member => text.literal("Mitglied"),
        OrgMessage::Owner => text.literal("Inhaber"),
        OrgMessage::Add => text.literal("Hinzufügen"),
        OrgMessage::MembersHint => text.literal(
            "Mitglieder erhalten die Basisberechtigung der Organisation für jedes Repository sowie alles, was ihre Teams gewähren. Inhaber können außerdem Mitglieder, Teams und Einstellungen verwalten.",
        ),
        OrgMessage::You => text.literal("(Sie)"),
        OrgMessage::Leave => text.literal("Verlassen"),
        OrgMessage::Remove => text.literal("Entfernen"),
        OrgMessage::MakeOwner => text.literal("Zum Inhaber machen"),
        OrgMessage::MakeMember => text.literal("Zum Mitglied machen"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" besitzt noch 1 Repository. Löschen oder übertragen Sie es zuerst.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" besitzt noch ");
            text.number(count);
            text.literal(" Repositorys. Löschen oder übertragen Sie sie zuerst.");
        }
        OrgText::RemoveLabel(name) => {
            text.argument(name);
            text.literal(" entfernen");
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal(": ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("Mitglieder · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("Einstellungen · ");
            text.argument(org);
        }
    }
}
