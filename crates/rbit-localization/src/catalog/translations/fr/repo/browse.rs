//! French (`fr`) entries for `catalog/repo/browse.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("À propos"),
        RepoBrowseLabel::NoDescription => text.literal("Aucune description."),
        RepoBrowseLabel::Visibility => text.literal("Visibilité"),
        RepoBrowseLabel::History => text.literal("Historique"),
        RepoBrowseLabel::CommitHistory => text.literal("Historique des commits"),
        RepoBrowseLabel::AddFile => text.literal("Ajouter un fichier"),
        RepoBrowseLabel::UploadFiles => text.literal("Téléverser des fichiers"),
        RepoBrowseLabel::Files => text.literal("Fichiers"),
        RepoBrowseLabel::ParentDirectory => text.literal("Dossier parent"),
        RepoBrowseLabel::Path => text.literal("Chemin"),
        RepoBrowseLabel::Latest => text.literal("Dernière"),
        RepoBrowseLabel::YourAccess => text.literal("Votre accès"),
        RepoBrowseLabel::LeaveRepository => text.literal("Quitter le dépôt"),
        RepoBrowseLabel::Languages => text.literal("Langages"),
        RepoBrowseLabel::RepositoryLinks => text.literal("Liens du dépôt"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Stocké avec Git LFS"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("Objet absent de ce serveur"),
        RepoBrowseLabel::EditThisFile => text.literal("Modifier ce fichier"),
        RepoBrowseLabel::DeleteThisFile => text.literal("Supprimer ce fichier"),
        RepoBrowseLabel::Blame => text.literal("Blame"),
        RepoBrowseLabel::Raw => text.literal("Brut"),
        RepoBrowseLabel::Download => text.literal("Télécharger"),
        RepoBrowseLabel::BinaryNotShown => text.literal("Fichier binaire non affiché."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("Ce fichier est trop volumineux pour être affiché."),
        RepoBrowseLabel::ViewRaw => text.literal("Afficher le fichier brut"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("Documents du dépôt"),
        RepoBrowseLabel::DocumentOutline => text.literal("Plan du document"),
        RepoBrowseLabel::FilterHeadings => text.literal("Filtrer les titres"),
        RepoBrowseLabel::DocumentHeadings => text.literal("Titres du document"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("Aucun titre correspondant."),
        RepoBrowseLabel::EmptyTitle => text.literal("Ce dépôt est vide"),
        RepoBrowseLabel::EmptyLead => text.literal("Poussez un projet existant ou démarrez-en un nouveau depuis la ligne de commande."),
        RepoBrowseLabel::RepositoryUrl => text.literal("URL du dépôt"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("Copier l’URL du dépôt"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("URL SSH du dépôt"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("Copier l’URL SSH du dépôt"),
        RepoBrowseLabel::EmptySshNote => text.literal(
            "Les commandes ci-dessous utilisent HTTPS. Pour pousser en SSH, utilisez plutôt l’URL SSH et ajoutez une clé dans ",
        ),
        RepoBrowseLabel::CreateNewRepository => text.literal("Créer un nouveau dépôt"),
        RepoBrowseLabel::PushExistingRepository => text.literal("Pousser un dépôt existant"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("Fermer l’explorateur de fichiers"),
        RepoBrowseLabel::LanguageFiles => text.literal("Fichiers du langage"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("Aucun fichier trouvé pour ce langage."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("Aucun fichier correspondant."),
        RepoBrowseLabel::FileContent => text.literal("Contenu du fichier"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("Sélectionnez un fichier pour en prévisualiser le contenu."),
        RepoBrowseLabel::SearchThisListing => text.literal("Rechercher dans cette liste"),
        RepoBrowseLabel::SearchNames => text.literal("Rechercher des noms"),
        RepoBrowseLabel::Search => text.literal("Rechercher"),
        RepoBrowseLabel::ListingPages => text.literal("Pages de la liste"),
        RepoBrowseLabel::Previous => text.literal("Précédent"),
        RepoBrowseLabel::Next => text.literal("Suivant"),
        RepoBrowseLabel::BranchesAndTags => text.literal("Branches et tags"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("Parcourir les branches et les tags"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("Trouver une branche ou un tag"),
        RepoBrowseLabel::Find => text.literal("Trouver"),
        RepoBrowseLabel::Branch => text.literal("Branche"),
        RepoBrowseLabel::Tag => text.literal("Tag"),
        RepoBrowseLabel::Default => text.literal("par défaut"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("Aucune branche ni aucun tag correspondant."),
        RepoBrowseLabel::NoForksYet => text.literal("Aucun fork pour l’instant"),
        RepoBrowseLabel::ForksEmpty => text.literal(
            "Les forks sont des copies de ce dépôt sur lesquelles d’autres personnes travaillent de manière indépendante.",
        ),
        RepoBrowseLabel::ForkLead => text.literal(
            "Un fork est votre propre copie des branches et des tags du dépôt. Il conserve la visibilité de l’original.",
        ),
        RepoBrowseLabel::ForkLeadPrivate => text.literal(
            "Un fork est votre propre copie des branches et des tags du dépôt. Il conserve la visibilité de l’original\u{a0}: \
             celui-ci restera privé.",
        ),
        RepoBrowseLabel::Owner => text.literal("Propriétaire"),
        RepoBrowseLabel::Name => text.literal("Nom"),
        RepoBrowseLabel::CreateFork => text.literal("Créer le fork"),
        RepoBrowseLabel::Cancel => text.literal("Annuler"),
        RepoBrowseLabel::NoStargazersYet => text.literal("Aucune étoile pour l’instant"),
        RepoBrowseLabel::StargazersEmpty => text.literal("Les personnes qui ajoutent une étoile à ce dépôt apparaissent ici."),
        RepoBrowseLabel::RepositoryActivity => text.literal("Activité du dépôt"),
        RepoBrowseLabel::GoToFile => text.literal("Aller au fichier"),
        RepoBrowseLabel::MatchingFiles => text.literal("Fichiers correspondants"),
        RepoBrowseLabel::LoadingFiles => text.literal("Chargement des fichiers…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("Impossible de charger les fichiers. Appuyez sur Entrée pour rechercher."),
        RepoBrowseLabel::SeeAllMatches => text.literal("Voir toutes les correspondances"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("Cette révision ne contient aucun fichier."),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("Cette révision contient trop de fichiers pour tous les parcourir."),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { count } => noun(text, count, "branche", "branches"),
        RepoBrowseMessage::TagNoun { count } => noun(text, count, "tag", "tags"),
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("Accès ");
            text.argument(permission);
            text.literal(" en tant que collaborateur.");
        }
        RepoBrowseMessage::LineCount { count } => {
            text.number(count as u64);
            noun(text, count, " ligne", " lignes");
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.literal("Fichiers ");
            text.argument(language);
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(total as u64);
            noun(text, total, " résultat · ", " résultats · ");
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
            noun(text, total, " référence", " références");
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.literal("Créer un fork de ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        RepoBrowseMessage::CommitNoun { count } => noun(text, count, "commit", "commits"),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("Aucun fichier ne correspond à « ");
            text.argument(query);
            text.literal(" ».");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.literal("Correspondances supplémentaires : ");
            text.number(count as u64);
            text.literal(". Saisissez une plus grande partie du nom pour affiner la liste.");
        }
    }
}

/// The singular or plural form, by `count`.
fn noun(text: &mut MessageWriter, count: usize, one: &str, other: &str) {
    text.literal(match text.plural(count as u64) {
        Plural::One => one,
        _ => other,
    });
}
