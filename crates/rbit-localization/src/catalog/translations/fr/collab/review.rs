//! French (`fr`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("Modifications"),
        ReviewMessage::ToggleFileTree => text.literal("Afficher ou masquer l’arborescence"),
        ReviewMessage::CollapseAll => text.literal("Tout réduire"),
        ReviewMessage::ExpandAll => text.literal("Tout développer"),
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commentaire obsolète se trouve dans la conversation",
                _ => " commentaires obsolètes se trouvent dans la conversation",
            });
        }
        ReviewMessage::FinishReview => text.literal("Terminer votre revue"),
        ReviewMessage::Truncated => text.literal(
            "Cette modification est très volumineuse\u{a0}: certains diffs ne sont pas affichés. Clonez le dépôt pour voir \
             la modification complète.",
        ),
        ReviewMessage::FileToggleHint => text.literal(
            "Cliquez pour réduire ou développer | Alt-clic pour tous les fichiers",
        ),
        ReviewMessage::ViewFile => text.literal("Afficher le fichier"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Commenter la ligne ");
            text.number(line);
        }
        ReviewMessage::StartReview => text.literal("Commencer une revue"),
        ReviewMessage::AddSingleComment => text.literal("Ajouter un commentaire isolé"),
        ReviewMessage::BinaryFile => text.literal("Fichier binaire non affiché."),
        ReviewMessage::TooLarge => text.literal("Diff trop volumineux pour être affiché."),
        ReviewMessage::SubmoduleCommit => text.literal("Commit de sous-module"),
        ReviewMessage::NoContentChanges => text.literal("Aucune modification du contenu."),
        ReviewMessage::NoChangesTitle => text.literal("Aucune modification"),
        ReviewMessage::NoChangesText => text.literal("La branche source n’apporte rien à la base."),
        ReviewMessage::Added => text.literal("Ajouté"),
        ReviewMessage::Deleted => text.literal("Supprimé"),
        ReviewMessage::Modified => text.literal("Modifié"),
        ReviewMessage::Renamed => text.literal("Renommé"),
        ReviewMessage::Copied => text.literal("Copié"),
        ReviewMessage::Outdated => text.literal("Obsolète"),
        ReviewMessage::Pending => text.literal("En attente"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("Conversation résolue · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("Répondre…"),
        ReviewMessage::AddToReview => text.literal("Ajouter à la revue"),
        ReviewMessage::ReopenConversation => text.literal("Rouvrir la conversation"),
        ReviewMessage::ResolveConversation => text.literal("Résoudre la conversation"),
        ReviewMessage::ReviewChanges => text.literal("Relire les modifications"),
        ReviewMessage::PendingCount { count } => {
            text.number(count);
            text.literal(" en attente");
        }
        ReviewMessage::ReviewPlaceholder => text.literal("Laissez un commentaire (Markdown pris en charge)"),
        ReviewMessage::RequestChanges => text.literal("Demander des modifications"),
        ReviewMessage::Approve => text.literal("Approuver"),
        ReviewMessage::DiscardReview => text.literal("Abandonner la revue en attente"),
        ReviewMessage::DismissReview => text.literal("Rejeter la revue"),
        ReviewMessage::DismissReason => text.literal("Pourquoi cette revue est-elle rejetée\u{202f}?"),
        ReviewMessage::Dismiss => text.literal("Rejeter"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("Rejetée par ");
            text.argument(by);
            text.literal("\u{a0}: ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("Relecteurs"),
        ReviewMessage::NoReviews => text.literal("Aucune revue pour l’instant"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Retirer la demande de revue adressée à ");
            text.argument(name);
        }
        ReviewMessage::RemoveRequest => text.literal("Retirer la demande"),
        ReviewMessage::RequestReviewFrom => text.literal("Demander une revue à"),
        ReviewMessage::Username => text.literal("nom d’utilisateur"),
        ReviewMessage::RequestReview => text.literal("Demander une revue"),
        ReviewMessage::Approved => text.literal("Approuvée"),
        ReviewMessage::ChangesRequested => text.literal("Modifications demandées"),
        ReviewMessage::Dismissed => text.literal("Rejetée"),
        ReviewMessage::Commented => text.literal("Commentée"),
        ReviewMessage::AwaitingReview => text.literal("En attente de revue"),
    }
}
