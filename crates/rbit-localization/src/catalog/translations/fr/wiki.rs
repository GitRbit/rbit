//! French (`fr`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;
use crate::plural::Plural;

entries! { Fr:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("Pages"),
        WikiMessage::AllPages => text.literal("Toutes les pages"),
        WikiMessage::PagesLabel => text.literal("Pages du wiki"),
        WikiMessage::BreadcrumbLabel => text.literal("Fil d’Ariane"),
        WikiMessage::SearchPages => text.literal("Rechercher des pages"),
        WikiMessage::Search => text.literal("Rechercher"),
        WikiMessage::NewPage => text.literal("Nouvelle page"),
        WikiMessage::EditPage => text.literal("Modifier la page"),
        WikiMessage::Edit => text.literal("Modifier"),
        WikiMessage::History => text.literal("Historique"),
        WikiMessage::Delete => text.literal("Supprimer"),
        WikiMessage::LastEdited => text.literal("Dernière modification"),
        WikiMessage::Revision { generation } => {
            text.literal("Révision ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("Révision"),
        WikiMessage::Author => text.literal("Auteur"),
        WikiMessage::Date => text.literal("Date"),
        WikiMessage::UnknownAuthor => text.literal("Auteur inconnu"),
        WikiMessage::Created => text.literal("Créée"),
        WikiMessage::Edited => text.literal("Modifiée"),
        WikiMessage::Renamed => text.literal("Renommée"),
        WikiMessage::Deleted => text.literal("Supprimée"),
        WikiMessage::Restored => text.literal("Restaurée"),
        WikiMessage::View => text.literal("Afficher"),
        WikiMessage::CompareWithPrevious => text.literal("Comparer avec la précédente"),
        WikiMessage::Restore => text.literal("Restaurer cette révision"),
        WikiMessage::OlderRevisions => text.literal("Révisions plus anciennes"),
        WikiMessage::OldRevisionNotice => text.literal("Vous consultez une ancienne révision de cette page."),
        WikiMessage::ViewCurrent => text.literal("Afficher la page actuelle"),
        WikiMessage::HistoryOf { title } => {
            text.literal("Historique de ");
            text.argument(title);
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("Modifications de la révision ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal(" à la révision ");
            text.number(u64::try_from(head).unwrap_or_default());
        }
        WikiMessage::Changes { additions, deletions } => {
            text.number(additions);
            text.literal(match text.plural(additions) {
                Plural::One => " ajout, ",
                _ => " ajouts, ",
            });
            text.number(deletions);
            text.literal(match text.plural(deletions) {
                Plural::One => " suppression",
                _ => " suppressions",
            });
        }
        WikiMessage::NoDifferences => text.literal("Ces révisions ont le même contenu."),
        WikiMessage::Title => text.literal("Titre"),
        WikiMessage::Path => text.literal("Chemin"),
        WikiMessage::PathHint => text.literal(
            "Lettres, chiffres, - _ et . séparés par /. Laissez vide pour le déduire du titre.",
        ),
        WikiMessage::Content => text.literal("Contenu"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("Enregistrer la page"),
        WikiMessage::Preview => text.literal("Aperçu"),
        WikiMessage::NothingToPreview => text.literal("Rien à prévisualiser."),
        WikiMessage::Cancel => text.literal("Annuler"),
        WikiMessage::TitleEmpty => text.literal("Saisissez un titre."),
        WikiMessage::TitleTooLong => text.literal("Le titre est trop long."),
        WikiMessage::TitleInvalid => text.literal("Le titre contient des caractères non autorisés."),
        WikiMessage::PathEmpty => text.literal("Saisissez un chemin, ou un titre dont il peut être déduit."),
        WikiMessage::PathTooLong => text.literal("Le chemin ou l’une de ses parties est trop long."),
        WikiMessage::PathTooDeep => text.literal("Le chemin comporte trop de parties."),
        WikiMessage::PathInvalid => text.literal(
            "Chaque partie du chemin doit commencer par une lettre ou un chiffre et ne contenir que des lettres, des \
             chiffres, - _ et .",
        ),
        WikiMessage::PathReserved => text.literal("Le chemin utilise un nom réservé."),
        WikiMessage::ContentTooLarge => text.literal("Le contenu est trop volumineux."),
        WikiMessage::ContentInvalid => text.literal("Le contenu contient des caractères non autorisés."),
        WikiMessage::Conflict => text.literal(
            "Cette page a été modifiée depuis que vous avez commencé. Votre texte est conservé ci-dessous\u{202f}; \
             consultez la page actuelle avant d’enregistrer de nouveau.",
        ),
        WikiMessage::CompareChanges => text.literal("Comparer les modifications"),
        WikiMessage::PathTaken => text.literal("Une autre page utilise déjà ce chemin."),
        WikiMessage::Unchanged => text.literal("Rien n’a changé."),
        WikiMessage::DeleteTitle { title } => {
            text.literal("Supprimer ");
            text.argument(title);
            text.literal("\u{202f}?");
        }
        WikiMessage::DeletePage => text.literal("Supprimer la page"),
        WikiMessage::Disabled => text.literal("Le wiki est désactivé pour ce dépôt."),
        WikiMessage::About => text.literal(
            "Un wiki contient des pages de documentation dotées de leur propre historique, distinctes du code.",
        ),
        WikiMessage::TurnOn => text.literal("Activer le wiki"),
        WikiMessage::TurnOff => text.literal("Désactiver le wiki"),
        WikiMessage::Empty => text.literal("Ce wiki n’a encore aucune page."),
        WikiMessage::CreateFirst => text.literal("Créer la première page"),
        WikiMessage::Missing => text.literal("Aucune page n’a ce chemin."),
        WikiMessage::CreatePath { path } => {
            text.literal("Créer ");
            text.argument(path);
        }
        WikiMessage::SearchCount { count, query } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " page correspond à «\u{a0}",
                _ => " pages correspondent à «\u{a0}",
            });
            text.argument(query);
            text.literal("\u{a0}»");
        }
        WikiMessage::NoResults { query } => {
            text.literal("Aucune page ne correspond à «\u{a0}");
            text.argument(query);
            text.literal("\u{a0}».");
        }
        WikiMessage::MorePages => text.literal("Plus de pages"),
    }
}
