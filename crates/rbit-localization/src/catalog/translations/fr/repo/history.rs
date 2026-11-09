//! French (`fr`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("Parcourir les fichiers à ce commit"),
        RepoHistoryLabel::BrowseFiles => text.literal("Parcourir les fichiers"),
        RepoHistoryLabel::Pagination => text.literal("Pagination"),
        RepoHistoryLabel::Newer => text.literal("Plus récents"),
        RepoHistoryLabel::Older => text.literal("Plus anciens"),
        RepoHistoryLabel::Committed => text.literal("a validé"),
        RepoHistoryLabel::CommittedBy => text.literal("validé par"),
        RepoHistoryLabel::Commit => text.literal("commit"),
        RepoHistoryLabel::Checks => text.literal("Vérifications"),
        RepoHistoryLabel::ViewFile => text.literal("Afficher le fichier"),
        RepoHistoryLabel::PathInThatCommit => text.literal("Chemin dans ce commit"),
        RepoHistoryLabel::FileEmpty => text.literal("Ce fichier est vide."),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("Le blame n’est pas disponible pour les fichiers binaires."),
        RepoHistoryLabel::TooLargeToBlame => text.literal("Ce fichier est trop volumineux pour le blame."),
        RepoHistoryLabel::ViewTheFile => text.literal("Afficher le fichier"),
        RepoHistoryLabel::CompareChanges => text.literal("Comparer les modifications"),
        RepoHistoryLabel::CompareLead => text.literal(
            "Choisissez une base, puis la branche, le tag ou le commit à comparer avec elle, y compris entre forks.",
        ),
        RepoHistoryLabel::Comparison => text.literal("Comparaison"),
        RepoHistoryLabel::MergeBase => text.literal("base de fusion"),
        RepoHistoryLabel::Swap => text.literal("Inverser"),
        RepoHistoryLabel::CreatePullRequest => text.literal("Créer une pull request"),
        RepoHistoryLabel::UnrelatedHistories => text.literal(
            "Ces révisions ont des historiques entièrement différents\u{a0}: il n’y a donc aucun diff à afficher.",
        ),
        RepoHistoryLabel::NothingToCompare => text.literal("Rien à comparer"),
        RepoHistoryLabel::NothingToCompareText => text.literal("Tout le contenu de la révision comparée se trouve déjà dans la base."),
        RepoHistoryLabel::Commits => text.literal("Commits"),
        RepoHistoryLabel::Compare => text.literal("Comparer"),
        RepoHistoryLabel::CompareWithDefault => text.literal("Comparer avec la branche par défaut"),
        RepoHistoryLabel::BranchView => text.literal("Affichage des branches"),
        RepoHistoryLabel::List => text.literal("Liste"),
        RepoHistoryLabel::GitTree => text.literal("Arbre Git"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("Aucune référence correspondante."),
        RepoHistoryLabel::NoBranchesYet => text.literal("Aucune branche pour l’instant"),
        RepoHistoryLabel::NoBranchesText => text.literal("Poussez un commit pour créer la première branche."),
        RepoHistoryLabel::NoTagsYet => text.literal("Aucun tag pour l’instant"),
        RepoHistoryLabel::NoTagsText => text.literal("Les tags marquent les releases et d’autres points importants de l’historique."),
        RepoHistoryLabel::Protected => text.literal("Protégée"),
        RepoHistoryLabel::ProtectedTitle => text.literal("Une règle de protection de branche s’applique à cette branche"),
        RepoHistoryLabel::DownloadTarGz => text.literal("Télécharger le tar.gz"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("Graphe des commits des branches"),
        RepoHistoryLabel::GraphHelp => text.literal(
            "Suivez les lignes colorées pour retracer l’historique. Sélectionnez une piste pour mettre ses commits en évidence.",
        ),
        RepoHistoryLabel::ClearHighlight => text.literal("Effacer la mise en évidence"),
        RepoHistoryLabel::HistoryConnections => text.literal("Liens de l’historique"),
        RepoHistoryLabel::CommitDetails => text.literal("Détails du commit"),
        RepoHistoryLabel::Merge => text.literal("Fusion"),
        RepoHistoryLabel::GraphTruncated => text.literal(
            "Affichage des 200\u{a0}derniers commits de toutes les branches. Les lignes du bas se poursuivent dans \
             l’historique plus ancien. Ouvrez l’historique d’une branche pour aller plus loin.",
        ),
        RepoHistoryLabel::LinesAdded => text.literal("Ajoutées"),
        RepoHistoryLabel::LinesDeleted => text.literal("Supprimées"),
        RepoHistoryLabel::StatusAdded => text.literal("Ajouté"),
        RepoHistoryLabel::StatusDeleted => text.literal("Supprimé"),
        RepoHistoryLabel::StatusModified => text.literal("Modifié"),
        RepoHistoryLabel::StatusRenamed => text.literal("Renommé"),
        RepoHistoryLabel::StatusCopied => text.literal("Copié"),
        RepoHistoryLabel::ChangedFiles => text.literal("Fichiers modifiés"),
        RepoHistoryLabel::FileTree => text.literal("Arborescence des fichiers"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("Filtrer les fichiers modifiés"),
        RepoHistoryLabel::FilterFiles => text.literal("Filtrer les fichiers…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("Aucun fichier ne correspond."),
        RepoHistoryLabel::Changes => text.literal("Modifications"),
        RepoHistoryLabel::ToggleFileTree => text.literal("Afficher ou masquer l’arborescence"),
        RepoHistoryLabel::CollapseAll => text.literal("Tout réduire"),
        RepoHistoryLabel::ExpandAll => text.literal("Tout développer"),
        RepoHistoryLabel::DiffTruncated => text.literal(
            "Cette modification est très volumineuse\u{a0}: certains diffs ne sont pas affichés. Clonez le dépôt pour voir \
             la modification complète.",
        ),
        RepoHistoryLabel::CollapseHint => text.literal(
            "Cliquez pour réduire ou développer · Alt-clic pour tous les fichiers",
        ),
        RepoHistoryLabel::CopyPath => text.literal("Copier le chemin"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("Afficher le fichier à ce commit"),
        RepoHistoryLabel::View => text.literal("Afficher"),
        RepoHistoryLabel::BinaryNotShown => text.literal("Fichier binaire non affiché."),
        RepoHistoryLabel::DiffTooLarge => text.literal("Diff trop volumineux pour être affiché."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("Commit de sous-module"),
        RepoHistoryLabel::NoContentChanges => text.literal("Aucune modification du contenu."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => labelled(text, "Commits du ", date),
        RepoHistoryMessage::ViewCommit { id } => labelled(text, "Afficher le commit ", id),
        RepoHistoryMessage::ParentNoun { count } => noun(text, count, "parent", "parents"),
        RepoHistoryMessage::BlameOf { file } => labelled(text, "Blame de ", file),
        RepoHistoryMessage::BlameTitle { path } => labelled(text, "Blame de ", path),
        RepoHistoryMessage::Ahead { count } => {
            text.argument(count);
            text.literal(" en avance");
        }
        RepoHistoryMessage::Behind { count } => {
            text.argument(count);
            text.literal(" en retard");
        }
        RepoHistoryMessage::OnlyNewest { count } => {
            text.literal("Seuls les ");
            text.number(count as u64);
            noun(text, count, " commit le plus récent est affiché.", " commits les plus récents sont affichés.");
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("Mettre en évidence la piste ");
            text.number(track as u64);
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("Piste ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name } => labelled(text, "Branche\u{a0}: ", name),
        RepoHistoryMessage::MoreBranches { count } => {
            text.number(count as u64);
            noun(text, count, " autre branche", " autres branches");
        }
        RepoHistoryMessage::FilesChangedNoun { count } => noun(text, count, "fichier modifié", "fichiers modifiés"),
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("Comparer ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}

/// Fixed wording followed by one value.
fn labelled(text: &mut MessageWriter, wording: &str, value: &str) {
    text.literal(wording);
    text.argument(value);
}

/// The singular or plural form, by `count`.
fn noun(text: &mut MessageWriter, count: usize, one: &str, other: &str) {
    text.literal(match text.plural(count as u64) {
        Plural::One => one,
        _ => other,
    });
}
