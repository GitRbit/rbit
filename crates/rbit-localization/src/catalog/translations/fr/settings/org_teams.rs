//! French (`fr`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::TeamSentence;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};
use crate::plural::Plural;

entries! { Fr:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("Nouvelle équipe"),
        TeamMessage::NoTeams => text.literal("Aucune équipe pour l’instant"),
        TeamMessage::NoTeamsText => text.literal(
            "Les équipes regroupent des membres et donnent à chaque groupe son propre accès aux dépôts.",
        ),
        TeamMessage::OneMember => text.literal("1\u{a0}membre"),
        TeamMessage::OneRepository => text.literal("1\u{a0}dépôt"),
        TeamMessage::CreateTeam => text.literal("Créer l’équipe"),
        TeamMessage::TeamSettings => text.literal("Paramètres de l’équipe"),
        TeamMessage::RenameHint => text.literal("Renommer l’équipe change son adresse."),
        TeamMessage::SaveChanges => text.literal("Enregistrer les modifications"),
        TeamMessage::DeleteTeam => text.literal("Supprimer cette équipe"),
        TeamMessage::DeleteTeamText => text.literal(
            "Ses membres restent dans l’organisation mais perdent l’accès que cette équipe leur donnait.",
        ),
        TeamMessage::DeleteTeamButton => text.literal("Supprimer l’équipe"),
        TeamMessage::UsernamePlaceholder => text.literal("Nom d’utilisateur"),
        TeamMessage::OnlyMembers => text.literal("Seuls les membres de l’organisation peuvent rejoindre ses équipes."),
        TeamMessage::NoMembers => text.literal("Aucun membre"),
        TeamMessage::NoMembersText => text.literal("Les membres obtiennent l’accès de cette équipe à ses dépôts."),
        TeamMessage::AddRepository => text.literal("Ajouter un dépôt"),
        TeamMessage::RepositoryPlaceholder => text.literal("Nom du dépôt"),
        TeamMessage::Permission => text.literal("Autorisation"),
        TeamMessage::RepositoryHint => text.literal(
            "N’importe quel dépôt de l’organisation. Les membres de l’équipe obtiennent cette autorisation sur celui-ci.",
        ),
        TeamMessage::NoRepositories => text.literal("Aucun dépôt"),
        TeamMessage::NoRepositoriesText => text.literal(
            "Cette équipe n’a accès à aucun dépôt que vous pouvez voir.",
        ),
        TeamMessage::Private => text.literal("Privé"),
        TeamMessage::Save => text.literal("Enregistrer"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " membre",
                _ => " membres",
            });
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dépôt",
                _ => " dépôts",
            });
        }
        TeamText::PermissionOn(repository) => {
            text.literal("Autorisation sur ");
            text.argument(repository);
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("Enregistrer l’autorisation sur ");
            text.argument(repository);
        }
        TeamText::TeamsTitle(org) => {
            text.literal("Équipes · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("Nouvelle équipe · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("Paramètres · ");
            text.argument(team);
        }
    }
    TeamSentence => RichWriter |text| {
        TeamSentence::NameHint => {
            text.literal("Lettres, chiffres, ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(" et ");
            text.slot(2);
            text.literal("\u{202f}; utilisé dans l’adresse de l’équipe.");
        }
    }
}
