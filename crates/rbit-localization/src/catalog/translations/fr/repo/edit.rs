//! French (`fr`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("Nouveau fichier"),
        RepoEditLabel::Edit => text.literal("Modifier"),
        RepoEditLabel::AfterName => text.literal(". "),
        RepoEditLabel::On => text.literal("Sur "),
        RepoEditLabel::NewFileHint => text.literal("Saisissez un chemin avec / pour créer des dossiers."),
        RepoEditLabel::EditHint => text.literal("Modifiez le chemin pour renommer ou déplacer le fichier."),
        RepoEditLabel::BackToFile => text.literal("Retour au fichier"),
        RepoEditLabel::Path => text.literal("Chemin"),
        RepoEditLabel::Content => text.literal("Contenu"),
        RepoEditLabel::CommitChanges => text.literal("Valider les modifications"),
        RepoEditLabel::Cancel => text.literal("Annuler"),
        RepoEditLabel::Delete => text.literal("Supprimer"),
        RepoEditLabel::DeleteHint => text.literal(
            "Le fichier reste dans l’historique\u{202f}; un dossier laissé vide disparaît.",
        ),
        RepoEditLabel::DeleteFile => text.literal("Supprimer le fichier"),
        RepoEditLabel::UploadFiles => text.literal("Téléverser des fichiers"),
        RepoEditLabel::Into => text.literal("Dans "),
        RepoEditLabel::OnBranch => text.literal(" sur "),
        RepoEditLabel::UploadHint => text.literal("Les fichiers de même nom sont remplacés."),
        RepoEditLabel::Files => text.literal("Fichiers"),
        RepoEditLabel::NothingCommitted => text.literal("Rien n’a été validé. "),
        RepoEditLabel::ReviewNewCommits => text.literal("Examinez les nouveaux commits"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal(
            ", puis validez de nouveau\u{a0}: vos modifications sont conservées ci-dessous et seront validées par-dessus la \
             nouvelle version.",
        ),
        RepoEditLabel::BranchGone => text.literal(
            "Rien n’a été validé. Validez plutôt sur une nouvelle branche\u{202f}; vos modifications sont conservées ci-dessous.",
        ),
        RepoEditLabel::CommitChangesLegend => text.literal("Valider les modifications"),
        RepoEditLabel::CommitMessage => text.literal("Message de commit"),
        RepoEditLabel::ExtendedDescription => text.literal("Description détaillée"),
        RepoEditLabel::Optional => text.literal("facultatif"),
        RepoEditLabel::CommitDirectlyTo => text.literal("Valider directement sur"),
        RepoEditLabel::NotAllowedEnd => text.literal("."),
        RepoEditLabel::NotAllowed => text.literal("Non autorisé\u{a0}: "),
        RepoEditLabel::NewBranchAndPull => text.literal("Créer une nouvelle branche et ouvrir une pull request"),
        RepoEditLabel::NewBranchName => text.literal("Nom de la nouvelle branche"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("Au plus ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("Jusqu’à ");
            text.number(files as u64);
            text.literal(match text.plural(files as u64) {
                Plural::One => " fichier d’au plus ",
                _ => " fichiers d’au plus ",
            });
            text.argument(size);
            text.literal(" chacun, validés ensemble.");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.argument(branch);
            text.literal(" a changé depuis que vous avez commencé");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" n’existe plus");
        }
        RepoEditMessage::NowPointsAt => text.literal("Elle pointe désormais sur "),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("Ce fichier est binaire ou dépasse ");
            text.argument(limit);
            text.literal(
                "\u{a0}: il ne peut donc pas être modifié dans le navigateur. Vous pouvez le supprimer ou téléverser un \
                 fichier de remplacement.",
            );
        }
        RepoEditMessage::EditingTitle { path } => {
            text.literal("Modification de ");
            text.argument(path);
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.literal("Supprimer ");
            text.argument(path);
        }
    }
}
