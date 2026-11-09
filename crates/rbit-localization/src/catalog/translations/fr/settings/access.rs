//! French (`fr`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Fr:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("Paramètres"),
        AccessMessage::Collaborators => text.literal("Collaborateurs"),
        AccessMessage::IntroLead => text.literal("Les personnes ajoutées ici ont accès à"),
        AccessMessage::IntroDirectly => text.literal("directement"),
        AccessMessage::IntroOrganization => text.literal(", qu’elles fassent partie ou non de l’organisation"),
        AccessMessage::IntroTail => text.literal("."),
        AccessMessage::AddCollaborator => text.literal("Ajouter un collaborateur"),
        AccessMessage::Username => text.literal("Nom d’utilisateur"),
        AccessMessage::Permission => text.literal("Autorisation"),
        AccessMessage::Add => text.literal("Ajouter"),
        AccessMessage::People => text.literal("Personnes"),
        AccessMessage::NoCollaborators => text.literal("Aucun collaborateur"),
        AccessMessage::NoCollaboratorsText => text.literal(
            "Ajoutez une personne par son nom d’utilisateur pour lui donner accès à ce dépôt.",
        ),
        AccessMessage::You => text.literal("(vous)"),
        AccessMessage::Save => text.literal("Enregistrer"),
        AccessMessage::Leave => text.literal("Quitter"),
        AccessMessage::Remove => text.literal("Retirer"),
        AccessMessage::Teams => text.literal("Équipes"),
        AccessMessage::ManageTeams => text.literal("Gérer les équipes"),
        AccessMessage::NoTeams => text.literal("Aucune équipe"),
        AccessMessage::NoTeamsText => text.literal(
            "Accordez l’accès à une équipe depuis la page de celle-ci\u{a0}; ses membres obtiennent alors cette \
             autorisation ici.",
        ),
        AccessMessage::Transfer => text.literal("Transférer la propriété"),
        AccessMessage::TransferLead => text.literal("Transférer"),
        AccessMessage::TransferTail => text.literal(
            "vers un autre compte. Son historique, ses collaborateurs et ses étoiles le suivent\u{202f}; les liens et \
             les URL de clonage qui utilisent l’ancienne adresse cessent de fonctionner, et les équipes de \
             l’organisation actuelle perdent leur accès.",
        ),
        AccessMessage::NewOwner => text.literal("Nouveau propriétaire"),
        AccessMessage::AdminEntry => text.literal(
            "En tant qu’administrateur du site, vous pouvez saisir n’importe quel utilisateur ou organisation.",
        ),
        AccessMessage::Organization => text.literal("Organisation"),
        AccessMessage::OwnerHint => text.literal("Votre compte, ou une organisation dont vous faites partie."),
        AccessMessage::TypeLead => text.literal("Saisissez"),
        AccessMessage::TypeTail => text.literal("pour confirmer"),
        AccessMessage::TransferRepository => text.literal("Transférer le dépôt"),
        AccessMessage::Nowhere => text.literal("Aucune destination possible"),
        AccessMessage::NowhereText => text.literal(
            "Vous ne faites partie d’aucune organisation où vous pourriez créer des dépôts, et ce dépôt vous \
             appartient déjà.",
        ),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.literal("Autorisation de ");
            text.argument(name);
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("Enregistrer l’autorisation de ");
            text.argument(name);
        }
        AccessText::RemoveLabel(name) => {
            text.literal("Retirer ");
            text.argument(name);
        }
        AccessText::Added(date) => {
            text.literal("Ajouté ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("Collaborateurs · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("Transfert · ");
            text.argument(repository);
        }
    }
}
