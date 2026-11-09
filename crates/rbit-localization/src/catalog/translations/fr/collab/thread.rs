//! French (`fr`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("Ouvert"),
        ThreadMessage::Closed => text.literal("Fermé"),
        ThreadMessage::Merged => text.literal("Fusionné"),
        ThreadMessage::Draft => text.literal("Brouillon"),
        ThreadMessage::Title => text.literal("Titre"),
        ThreadMessage::Description => text.literal("Description"),
        ThreadMessage::Edit => text.literal("Modifier"),
        ThreadMessage::Save => text.literal("Enregistrer"),
        ThreadMessage::Cancel => text.literal("Annuler"),
        ThreadMessage::Delete => text.literal("Supprimer"),
        ThreadMessage::Optional => text.literal("facultatif"),
        ThreadMessage::MarkdownSupported => text.literal("Markdown pris en charge"),
        ThreadMessage::SignIn => text.literal("Connectez-vous"),
        ThreadMessage::LoadMore => text.literal("Charger plus"),
        ThreadMessage::AddComment => text.literal("Ajouter un commentaire"),
        ThreadMessage::CommentPlaceholder => text.literal(
            "Écrivez un commentaire. Utilisez @ pour les utilisateurs et # pour les tickets ou les pull requests.",
        ),
        ThreadMessage::MentionSuggestions => text.literal("Suggestions de mentions"),
        ThreadMessage::CommentHint => text.literal(
            "Markdown pris en charge. Utilisez @nom, #123 ou propriétaire/dépôt#123 pour créer un lien vers un \
             utilisateur, un ticket ou une pull request.",
        ),
        ThreadMessage::LockedHint => text.literal(
            "Cette conversation est verrouillée\u{202f}; seuls les collaborateurs peuvent commenter.",
        ),
        ThreadMessage::CloseIssue => text.literal("Fermer le ticket"),
        ThreadMessage::ClosePull => text.literal("Fermer la pull request"),
        ThreadMessage::CloseNotPlanned => text.literal("Fermer comme non prévu"),
        ThreadMessage::ReopenIssue => text.literal("Rouvrir le ticket"),
        ThreadMessage::ReopenPull => text.literal("Rouvrir la pull request"),
        ThreadMessage::Comment => text.literal("Commenter"),
        ThreadMessage::LockConversation => text.literal("Verrouiller la conversation"),
        ThreadMessage::UnlockConversation => text.literal("Déverrouiller la conversation"),
        ThreadMessage::Bot => text.literal("bot"),
        ThreadMessage::Edited => text.literal("modifié"),
        ThreadMessage::Author => text.literal("Auteur"),
        ThreadMessage::Reply => text.literal("Répondre"),
        ThreadMessage::DeleteCommentConfirm => text.literal("Supprimer ce commentaire\u{202f}?"),
        ThreadMessage::DeleteComment => text.literal("Supprimer le commentaire"),
        ThreadMessage::NoDescription => text.literal("Aucune description fournie."),
    }
}
