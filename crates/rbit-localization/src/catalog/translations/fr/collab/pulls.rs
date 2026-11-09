//! French (`fr`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("Pull requests"),
        PullMessage::SearchPulls => text.literal("Rechercher des pull requests"),
        PullMessage::ClearSearch => text.literal("Effacer la recherche et les filtres"),
        PullMessage::FiltersLabel => text.literal("Filtres des pull requests"),
        PullMessage::StateNav => text.literal("État des pull requests"),
        PullMessage::AuthorFilter { author } => {
            text.literal("auteur\u{a0}: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("Nouvelle"),
        PullMessage::NewSuffix => text.literal("pull request"),
        PullMessage::EmptyTitle => text.literal("Aucune pull request ici"),
        PullMessage::EmptyText => text.literal(
            "Les pull requests proposent des modifications depuis une branche ou un fork. Comparez des branches pour en \
             ouvrir une.",
        ),
        PullMessage::OpenTitle => text.literal("Ouvrir une pull request"),
        PullMessage::OpenIntro => text.literal(
            "Proposez les modifications d’une branche, de ce dépôt ou d’un fork, pour la branche de base.",
        ),
        PullMessage::OpenAsDraft => text.literal("Ouvrir comme brouillon"),
        PullMessage::Create => text.literal("Créer la pull request"),
        PullMessage::DifferentHistories => text.literal("Ces branches ont des historiques entièrement différents."),
        PullMessage::ChooseBranches => text.literal("Choisissez deux branches existantes à comparer."),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" contient déjà tous les commits de ");
            text.argument(head);
            text.literal(".");
        }
        PullMessage::BaseRepository => text.literal("Dépôt de base"),
        PullMessage::BaseRefs => text.literal("Branches et tags de base"),
        PullMessage::Base => text.literal("Base"),
        PullMessage::HeadRepository => text.literal("Dépôt source"),
        PullMessage::Compare => text.literal("Comparer"),
        PullMessage::CompareRefs => text.literal("Comparer des branches et des tags"),
        PullMessage::PullRequest => text.literal("Pull request"),
        PullMessage::Conversation => text.literal("Conversation"),
        PullMessage::Commits => text.literal("Commits"),
        PullMessage::FilesChanged => text.literal("Fichiers modifiés"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commit",
                _ => " commits",
            });
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("des commits"),
        PullMessage::ChangeBase => text.literal("Changer la branche de base"),
        PullMessage::BaseBranch => text.literal("Branche de base"),
        PullMessage::BaseBranches => text.literal("Branches de base"),
        PullMessage::ChangeBaseHint => text.literal(
            "Changer la base met à jour les commits, le diff et les vérifications de fusion.",
        ),
        PullMessage::UpdateBase => text.literal("Mettre à jour la branche de base"),
        PullMessage::IsClosed => text.literal("Cette pull request est fermée."),
        PullMessage::IsMerged => text.literal("Pull request fusionnée."),
        PullMessage::DeleteBranch { branch } => {
            text.literal("Supprimer ");
            text.argument(branch);
        }
        PullMessage::ConvertToDraft => text.literal("Convertir en brouillon"),
        PullMessage::NoCommitsTitle => text.literal("Aucun commit"),
        PullMessage::NoCommitsText => text.literal("La branche source et la base pointent sur le même commit."),
        PullMessage::NewestCommits { count } => {
            text.literal("Seuls les ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " commit le plus récent est affiché.",
                _ => " commits les plus récents sont affichés.",
            });
        }
    }
}
