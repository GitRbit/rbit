//! French (`fr`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};
use crate::plural::Plural;

entries! { Fr:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("Rechercher dans les dépôts publics"),
        HomeMessage::Search => text.literal("Rechercher"),
        HomeMessage::ExploreAll => text.literal("Explorer tous les dépôts"),
        HomeMessage::ThisInstance => text.literal("Cette instance"),
        HomeMessage::RepositoryCount(count) => label(text, count.unsigned_abs(), "Dépôt", "Dépôts"),
        HomeMessage::UserCount(count) => label(text, count.unsigned_abs(), "Utilisateur", "Utilisateurs"),
        HomeMessage::OrganizationCount(count) => label(text, count.unsigned_abs(), "Organisation", "Organisations"),
        HomeMessage::RecentlyUpdated => text.literal("Mis à jour récemment"),
        HomeMessage::ViewAll => text.literal("Tout afficher"),
        HomeMessage::NoPublicRepositories => text.literal("Aucun dépôt public pour l’instant"),
        HomeMessage::NoPublicRepositoriesText => text.literal("Les dépôts rendus publics sur ce serveur apparaîtront ici."),
        HomeMessage::FreshServer => text.literal("Ce serveur n’a encore aucun compte. Le premier compte inscrit en devient l’administrateur."),
        HomeMessage::BuiltForSelfHosting => text.literal("Conçu pour l’auto-hébergement"),
        HomeMessage::FeatureTitle(feature) => text.literal(feature_title(feature)),
        HomeMessage::FeatureText(feature) => text.literal(feature_text(feature)),
        HomeMessage::Dashboard => text.literal("Tableau de bord"),
        HomeMessage::ViewProfile => text.literal("Voir le profil"),
        HomeMessage::New => text.literal("Nouveau"),
        HomeMessage::NewRepository => text.literal("Nouveau dépôt"),
        HomeMessage::PersonalDashboards => text.literal("Tableaux de bord personnels"),
        HomeMessage::Issues => text.literal("Tickets"),
        HomeMessage::PullRequests => text.literal("Pull requests"),
        HomeMessage::Repositories => text.literal("Dépôts"),
        HomeMessage::PrivateCount(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " privé",
                _ => " privés",
            });
        }
        HomeMessage::FindRepository => text.literal("Trouver un dépôt…"),
        HomeMessage::FilterRepositories => text.literal("Filtrer vos dépôts"),
        HomeMessage::NoRepositories => text.literal("Aucun dépôt pour l’instant"),
        HomeMessage::NoRepositoriesText => text.literal("Créez un dépôt pour commencer à héberger du code sur ce serveur."),
        HomeMessage::CreateRepository => text.literal("Créer un dépôt"),
        HomeMessage::NoFilterMatch => text.literal("Aucun dépôt ne correspond à votre filtre."),
        HomeMessage::Organizations => text.literal("Organisations"),
        HomeMessage::NewOrganization => text.literal("Nouvelle organisation"),
        HomeMessage::NotAMember => text.literal("Vous n’êtes membre d’aucune organisation."),
        HomeMessage::CreateOne => text.literal("Créez-en une"),
        HomeMessage::ShareWithTeam => text.literal("pour partager des dépôts avec une équipe."),
        HomeMessage::Explore => text.literal("Explorer"),
        HomeMessage::NothingPublic => text.literal("Rien de public pour l’instant."),
        HomeMessage::YourInvolvement => text.literal("Votre participation"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "Créés par moi",
            Involvement::Assigned => "Assignés",
            Involvement::Mentioned => "Mentionné",
            Involvement::ReviewRequested => "Revue demandée",
        }),
        HomeMessage::SearchIn(title) => {
            text.literal("Rechercher\u{a0}: ");
            text.argument(title);
        }
        HomeMessage::SearchHelp => text.literal("Aide à la recherche"),
        HomeMessage::SearchHelpText => text.literal(
            "Utilisez is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee ou \
             sort:updated-desc. Le texte est recherché dans les titres et les descriptions. Les résultats ne \
             comprennent que les dépôts que vous pouvez lire actuellement. «\u{a0}Mentionné\u{a0}» inclut les \
             mentions connues dans les descriptions, les commentaires et les revues envoyées, même une fois les \
             notifications lues.",
        ),
        HomeMessage::Results => text.literal("Résultats"),
        HomeMessage::OpenCount(count) => counted(text, count, " ouvert", " ouverts"),
        HomeMessage::ClosedCount(count) => counted(text, count, " fermé", " fermés"),
        HomeMessage::NoMatchingThreads => text.literal("Aucune conversation correspondante"),
        HomeMessage::NoMatchingThreadsText => text.literal("Essayez un autre filtre ou un autre onglet de participation."),
        HomeMessage::CommentCount(count) => counted(text, u64::from(count), " commentaire", " commentaires"),
    }
}

/// A figure's label, singular or plural by the figure.
fn label(text: &mut MessageWriter, count: u64, one: &str, other: &str) {
    text.literal(match text.plural(count) {
        Plural::One => one,
        _ => other,
    });
}

/// The count followed by its noun.
fn counted(text: &mut MessageWriter, count: u64, one: &str, other: &str) {
    text.number(count);
    label(text, count, one, other);
}

const fn feature_title(feature: Feature) -> &'static str {
    match feature {
        Feature::Binary => "Un seul binaire rapide",
        Feature::Protocols => "Git en HTTPS et en SSH",
        Feature::Teams => "Organisations et équipes",
        Feature::Releases => "Releases, miroirs et webhooks",
        Feature::Security => "Sécurisé par défaut",
        Feature::Screens => "Sur tous les écrans",
    }
}

const fn feature_text(feature: Feature) -> &'static str {
    match feature {
        Feature::Binary => "SQLite et un stockage intégré. Aucun autre service à installer ni à exécuter.",
        Feature::Protocols => "Smart HTTP avec le protocole v2, un serveur SSH intégré et des clés de déploiement.",
        Feature::Teams => "Partagez des dépôts avec des équipes et des collaborateurs, rôle par rôle.",
        Feature::Releases => "Publiez des releases, mettez en miroir des projets en amont et notifiez d’autres services.",
        Feature::Security => "Connexion à deux facteurs, jetons d’accès à portée limitée et journal d’audit.",
        Feature::Screens => "S’adapte jusqu’aux écrans de téléphone et s’installe comme une application.",
    }
}
