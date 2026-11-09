//! French (`fr`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};
use crate::plural::Plural;

entries! { Fr:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("Choisissez une autorisation."),
        OrgMessage::Organization => text.literal("Organisation"),
        OrgMessage::Repositories => text.literal("Dépôts"),
        OrgMessage::Members => text.literal("Membres"),
        OrgMessage::Teams => text.literal("Équipes"),
        OrgMessage::Settings => text.literal("Paramètres"),
        OrgMessage::NewOrganization => text.literal("Nouvelle organisation"),
        OrgMessage::NewIntro => text.literal(
            "Une organisation possède des dépôts pour le compte d’une équipe. Vous en serez le premier propriétaire.",
        ),
        OrgMessage::Name => text.literal("Nom"),
        OrgMessage::NameHint => text.literal(
            "Utilisé dans les URL. Lettres, chiffres, «\u{a0}-\u{a0}», «\u{a0}_\u{a0}» et «\u{a0}.\u{a0}». Les \
             utilisateurs et les organisations partagent un même espace de noms.",
        ),
        OrgMessage::DisplayName => text.literal("Nom affiché"),
        OrgMessage::Optional => text.literal("facultatif"),
        OrgMessage::Description => text.literal("Description"),
        OrgMessage::Cancel => text.literal("Annuler"),
        OrgMessage::CreateOrganization => text.literal("Créer l’organisation"),
        OrgMessage::New => text.literal("Nouveau"),
        OrgMessage::NoRepositories => text.literal("Aucun dépôt pour l’instant"),
        OrgMessage::NoRepositoriesText => text.literal("Les dépôts appartenant à cette organisation apparaîtront ici."),
        OrgMessage::Profile => text.literal("Profil"),
        OrgMessage::Website => text.literal("Site web"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("Enregistrer le profil"),
        OrgMessage::MemberPermission => text.literal("Autorisation des membres"),
        OrgMessage::PermissionLegend => text.literal("Ce que chaque membre obtient sur chaque dépôt"),
        OrgMessage::PermissionHint => text.literal(
            "Les équipes et les collaborateurs peuvent y ajouter des droits, jamais en retirer. Les propriétaires \
             administrent toujours tous les dépôts.",
        ),
        OrgMessage::SavePermission => text.literal("Enregistrer l’autorisation"),
        OrgMessage::NoPermission => text.literal("Aucune autorisation"),
        OrgMessage::NoPermissionText => text.literal(
            "Les membres ne voient que les dépôts publics et ceux auxquels leurs équipes ont accès.",
        ),
        OrgMessage::Read => text.literal("Lecture"),
        OrgMessage::ReadText => text.literal("Les membres peuvent parcourir et cloner tous les dépôts."),
        OrgMessage::Write => text.literal("Écriture"),
        OrgMessage::WriteText => text.literal("Les membres peuvent parcourir, cloner et pousser vers tous les dépôts."),
        OrgMessage::DangerZone => text.literal("Zone de danger"),
        OrgMessage::DeleteOrganization => text.literal("Supprimer cette organisation"),
        OrgMessage::DeleteText => text.literal(
            "Cette action supprime définitivement l’organisation et toutes ses appartenances. Elle est irréversible.",
        ),
        OrgMessage::TypeToConfirmLead => text.literal("Saisissez"),
        OrgMessage::TypeToConfirmTail => text.literal("pour confirmer"),
        OrgMessage::DeleteOrganizationButton => text.literal("Supprimer l’organisation"),
        OrgMessage::AddMember => text.literal("Ajouter un membre"),
        OrgMessage::Username => text.literal("Nom d’utilisateur"),
        OrgMessage::Role => text.literal("Rôle"),
        OrgMessage::Member => text.literal("Membre"),
        OrgMessage::Owner => text.literal("Propriétaire"),
        OrgMessage::Add => text.literal("Ajouter"),
        OrgMessage::MembersHint => text.literal(
            "Les membres obtiennent l’autorisation de base de l’organisation sur chaque dépôt, plus ce que leurs \
             équipes leur accordent. Les propriétaires peuvent aussi gérer les membres, les équipes et les paramètres.",
        ),
        OrgMessage::You => text.literal("(vous)"),
        OrgMessage::Leave => text.literal("Quitter"),
        OrgMessage::Remove => text.literal("Retirer"),
        OrgMessage::MakeOwner => text.literal("Nommer propriétaire"),
        OrgMessage::MakeMember => text.literal("Rétrograder en membre"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" possède encore 1\u{a0}dépôt. Supprimez-le ou transférez-le d’abord.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" possède encore ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dépôt. Supprimez-le ou transférez-le d’abord.",
                _ => " dépôts. Supprimez-les ou transférez-les d’abord.",
            });
        }
        OrgText::RemoveLabel(name) => {
            text.literal("Retirer ");
            text.argument(name);
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal("\u{a0}: ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("Membres · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("Paramètres · ");
            text.argument(org);
        }
    }
}
