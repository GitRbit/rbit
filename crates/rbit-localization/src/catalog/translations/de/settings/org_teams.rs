//! German (`de`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};
use crate::plural::Plural;

entries! { De:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("Neues Team"),
        TeamMessage::NoTeams => text.literal("Noch keine Teams"),
        TeamMessage::NoTeamsText => text.literal(
            "Teams fassen Mitglieder zusammen und geben jeder Gruppe ihren eigenen Zugriff auf Repositorys.",
        ),
        TeamMessage::OneMember => text.literal("1 Mitglied"),
        TeamMessage::OneRepository => text.literal("1 Repository"),
        TeamMessage::NameHintLead => text.literal("Buchstaben, Ziffern,"),
        TeamMessage::NameHintAnd => text.literal("und"),
        TeamMessage::NameHintTail => text.literal("; wird in der Adresse des Teams verwendet."),
        TeamMessage::CreateTeam => text.literal("Team erstellen"),
        TeamMessage::TeamSettings => text.literal("Team-Einstellungen"),
        TeamMessage::RenameHint => text.literal("Eine Umbenennung ändert die Adresse des Teams."),
        TeamMessage::SaveChanges => text.literal("Änderungen speichern"),
        TeamMessage::DeleteTeam => text.literal("Dieses Team löschen"),
        TeamMessage::DeleteTeamText => text.literal(
            "Seine Mitglieder bleiben in der Organisation, verlieren aber den Zugriff, den ihnen dieses Team gegeben hat.",
        ),
        TeamMessage::DeleteTeamButton => text.literal("Team löschen"),
        TeamMessage::UsernamePlaceholder => text.literal("Benutzername"),
        TeamMessage::OnlyMembers => text.literal(
            "Nur Mitglieder der Organisation können ihren Teams beitreten.",
        ),
        TeamMessage::NoMembers => text.literal("Keine Mitglieder"),
        TeamMessage::NoMembersText => text.literal(
            "Mitglieder erhalten den Zugriff dieses Teams auf seine Repositorys.",
        ),
        TeamMessage::AddRepository => text.literal("Repository hinzufügen"),
        TeamMessage::RepositoryPlaceholder => text.literal("Repository-Name"),
        TeamMessage::Permission => text.literal("Berechtigung"),
        TeamMessage::RepositoryHint => text.literal(
            "Ein beliebiges Repository der Organisation. Mitglieder des Teams erhalten diese Berechtigung dafür.",
        ),
        TeamMessage::NoRepositories => text.literal("Keine Repositorys"),
        TeamMessage::NoRepositoriesText => text.literal(
            "Dieses Team hat keinen Zugriff auf ein Repository erhalten, das Sie sehen können.",
        ),
        TeamMessage::Private => text.literal("Privat"),
        TeamMessage::Save => text.literal("Speichern"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Mitglied",
                _ => " Mitglieder",
            });
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Repository",
                _ => " Repositorys",
            });
        }
        TeamText::PermissionOn(repository) => {
            text.literal("Berechtigung für ");
            text.argument(repository);
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("Berechtigung für ");
            text.argument(repository);
            text.literal(" speichern");
        }
        TeamText::TeamsTitle(org) => {
            text.literal("Teams · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("Neues Team · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("Einstellungen · ");
            text.argument(team);
        }
    }
}
