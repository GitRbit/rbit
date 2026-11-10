//! French (`fr`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};
use crate::plural::Plural;

entries! { Fr:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("Toutes les personnes ayant un compte sur cette instance."),
        AdminPageMessage::UsersSearch => text.literal("Rechercher par nom d’utilisateur, nom ou adresse e-mail"),
        AdminPageMessage::NewUser => text.literal("Nouvel utilisateur"),
        AdminPageMessage::NoUsers => text.literal("Aucun utilisateur trouvé"),
        AdminPageMessage::NoUsersText => text.literal("Essayez une autre recherche."),
        AdminPageMessage::User => text.literal("Utilisateur"),
        AdminPageMessage::Joined => text.literal("Inscription"),
        AdminPageMessage::Admin => text.literal("Administrateur"),
        AdminPageMessage::Suspended => text.literal("Suspendu"),
        AdminPageMessage::EmailVerified => text.literal("Adresse e-mail vérifiée"),
        AdminPageMessage::Verified => text.literal("Vérifiée"),
        AdminPageMessage::You => text.literal("Vous"),
        AdminPageMessage::RemoveAdmin => text.literal("Retirer les droits d’administrateur"),
        AdminPageMessage::MakeAdmin => text.literal("Nommer administrateur"),
        AdminPageMessage::LiftSuspension => text.literal("Lever la suspension"),
        AdminPageMessage::Suspend => text.literal("Suspendre"),
        AdminPageMessage::SendReset => text.literal("Envoyer une réinitialisation du mot de passe"),
        AdminPageMessage::SetPasswordMenu => text.literal("Définir le mot de passe…"),
        AdminPageMessage::DeleteMenu => text.literal("Supprimer…"),
        AdminPageMessage::ResetSent => text.literal("E-mail de réinitialisation du mot de passe envoyé."),
        AdminPageMessage::PasswordSet => text.literal("Mot de passe mis à jour."),
        AdminPageMessage::NewUserIntro => text.literal(
            "Les administrateurs peuvent créer des comptes même lorsque les inscriptions sont fermées.",
        ),
        AdminPageMessage::Username => text.literal("Nom d’utilisateur"),
        AdminPageMessage::UsernameHint => text.literal("Lettres, chiffres, «\u{a0}-\u{a0}», «\u{a0}_\u{a0}» et «\u{a0}.\u{a0}»"),
        AdminPageMessage::Email => text.literal("Adresse e-mail"),
        AdminPageMessage::InitialPassword => text.literal("Mot de passe initial"),
        AdminPageMessage::SiteAdministrator => text.literal("Administrateur du site"),
        AdminPageMessage::SiteAdministratorText => text.literal("Accès complet à tous les dépôts et à ce panneau."),
        AdminPageMessage::CreateUser => text.literal("Créer l’utilisateur"),
        AdminPageMessage::RegistrationClosed => text.literal(
            "Les inscriptions publiques sont actuellement fermées\u{202f}; c’est le seul moyen d’ajouter des utilisateurs.",
        ),
        AdminPageMessage::SetPassword => text.literal("Définir le mot de passe"),
        AdminPageMessage::ResetHint => text.literal(
            "Pour lui permettre de choisir son propre mot de passe, envoyez plutôt une réinitialisation depuis la liste \
             des utilisateurs.",
        ),
        AdminPageMessage::DeleteUser => text.literal("Supprimer l’utilisateur"),
        AdminPageMessage::DeleteUserLead => text.literal("Cette action supprime définitivement"),
        AdminPageMessage::OrganizationsIntro => text.literal("Les espaces de noms partagés et leurs membres."),
        AdminPageMessage::SearchByName => text.literal("Rechercher par nom"),
        AdminPageMessage::NoOrganizations => text.literal("Aucune organisation trouvée"),
        AdminPageMessage::NoOrganizationsText => text.literal("Les organisations apparaîtront ici dès qu’elles existeront."),
        AdminPageMessage::Organization => text.literal("Organisation"),
        AdminPageMessage::Members => text.literal("Membres"),
        AdminPageMessage::Created => text.literal("Création"),
        AdminPageMessage::RepositoriesIntro => text.literal(
            "Tous les dépôts, publics et privés, les plus récemment actifs en premier.",
        ),
        AdminPageMessage::RepositoriesSearch => text.literal("Rechercher par propriétaire/nom ou description"),
        AdminPageMessage::NoRepositories => text.literal("Aucun dépôt trouvé"),
        AdminPageMessage::NoRepositoriesText => text.literal("Les dépôts apparaîtront ici dès qu’ils existeront."),
        AdminPageMessage::Repository => text.literal("Dépôt"),
        AdminPageMessage::Updated => text.literal("Mise à jour"),
        AdminPageMessage::DeleteRepository => text.literal("Supprimer le dépôt"),
        AdminPageMessage::DeleteRepositoryText => text.literal(
            "Cette action supprime définitivement le dépôt, y compris toutes ses données Git. Elle est irréversible.",
        ),
        AdminPageMessage::AuditIntro => text.literal(
            "Connexions, modifications de l’authentification à deux facteurs, sessions, jetons d’accès et actions des \
             administrateurs, les plus récents en premier.",
        ),
        AdminPageMessage::AuditSearch => text.literal("Utilisateur, cible ou adresse IP"),
        AdminPageMessage::Category => text.literal("Catégorie"),
        AdminPageMessage::Filter => text.literal("Filtrer"),
        AdminPageMessage::AllEvents => text.literal("Tous les événements"),
        AdminPageMessage::Clear => text.literal("Effacer"),
        AdminPageMessage::NoEvents => text.literal("Aucun événement"),
        AdminPageMessage::NoEventsText => text.literal("Rien ne correspond encore à ces filtres."),
        AdminPageMessage::As => text.literal("en tant que"),
        AdminPageMessage::By => text.literal("par"),
        AdminPageMessage::On => text.literal("sur"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("Les modifications s’appliquent immédiatement à toute l’instance."),
        InstanceSettingsMessage::Saved => text.literal("Paramètres enregistrés."),
        InstanceSettingsMessage::SiteName => text.literal("Nom du site"),
        InstanceSettingsMessage::SiteNameHint => text.literal(
            "Affiché dans l’en-tête, dans les titres de page et lorsque le site est installé comme application.",
        ),
        InstanceSettingsMessage::Branding => text.literal("Image de marque et identité"),
        InstanceSettingsMessage::BrandingHint => text.literal(
            "Le texte est du texte brut. Les images et les liens acceptent des URL HTTPS ou des /chemins locaux servis \
             par votre proxy inverse. Les champs d’image vides utilisent les images fournies par défaut.",
        ),
        InstanceSettingsMessage::PrimaryColor => text.literal("Couleur du bouton principal (vide\u{a0}: celle du thème)"),
        InstanceSettingsMessage::PrimaryTextColor => text.literal("Couleur du texte du bouton principal"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => text.literal(
            "Choisissez des couleurs suffisamment contrastées pour que le texte des boutons reste lisible. Définissez \
             les deux couleurs ou laissez-les toutes deux vides.",
        ),
        InstanceSettingsMessage::Description => text.literal("Description du site"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("Titre d’accueil (vide\u{a0}: le nom du site)"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("Message d’accueil"),
        InstanceSettingsMessage::LogoUrl => text.literal("URL du logo"),
        InstanceSettingsMessage::FaviconUrl => text.literal("URL de la favicon"),
        InstanceSettingsMessage::AppIconUrl => text.literal("URL de l’icône d’application (carrée, 512\u{a0}pixels minimum)"),
        InstanceSettingsMessage::FooterText => text.literal("Texte du pied de page"),
        InstanceSettingsMessage::SupportUrl => text.literal("URL d’assistance"),
        InstanceSettingsMessage::TermsUrl => text.literal("URL des conditions d’utilisation"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("URL de la politique de confidentialité"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("Afficher «\u{a0}Powered by rbit\u{a0}»"),
        InstanceSettingsMessage::ShowVersion => text.literal("Afficher la version du logiciel"),
        InstanceSettingsMessage::ShowFeatures => text.literal(
            "Afficher les fonctionnalités d’auto-hébergement sur la page d’accueil",
        ),
        InstanceSettingsMessage::Explore => text.literal("Page Explorer"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("Activée"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("Toute personne pouvant naviguer sur le site peut rechercher et lister tous les dépôts publics."),
        InstanceSettingsMessage::ExploreDisabled => text.literal("Désactivée"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("La page Explorer est introuvable et ses liens sont masqués, y compris pour les administrateurs. Les dépôts publics restent publics."),
        InstanceSettingsMessage::Browsing => text.literal("Qui peut naviguer"),
        InstanceSettingsMessage::BrowsingOpen => text.literal("Tout le monde"),
        InstanceSettingsMessage::BrowsingOpenText => text.literal("Les visiteurs peuvent consulter les dépôts, profils et organisations publics sans compte."),
        InstanceSettingsMessage::BrowsingSignedIn => text.literal("Utilisateurs connectés uniquement"),
        InstanceSettingsMessage::BrowsingSignedInText => text.literal("Les visiteurs sont redirigés vers la page de connexion. Git, LFS et l’API refusent les lectures sans identifiants."),
        InstanceSettingsMessage::Registration => text.literal("Inscriptions"),
        InstanceSettingsMessage::Open => text.literal("Ouvertes"),
        InstanceSettingsMessage::OpenText => text.literal("Tout le monde peut créer un compte depuis la page d’inscription."),
        InstanceSettingsMessage::Closed => text.literal("Fermées"),
        InstanceSettingsMessage::ClosedText => text.literal("Seuls les administrateurs peuvent créer des comptes."),
        InstanceSettingsMessage::OrgCreation => text.literal("Qui peut créer des organisations"),
        InstanceSettingsMessage::AnyUser => text.literal("Tout utilisateur"),
        InstanceSettingsMessage::AnyUserText => text.literal("Tout utilisateur connecté peut créer des organisations."),
        InstanceSettingsMessage::AdminsOnly => text.literal("Administrateurs uniquement"),
        InstanceSettingsMessage::AdminsOnlyText => text.literal(
            "Les utilisateurs peuvent toujours être ajoutés à des organisations existantes.",
        ),
        InstanceSettingsMessage::DefaultVisibility => text.literal("Visibilité par défaut des dépôts"),
        InstanceSettingsMessage::Public => text.literal("Public"),
        InstanceSettingsMessage::PublicText => text.literal(
            "Les nouveaux dépôts sont visibles par tous, sauf modification.",
        ),
        InstanceSettingsMessage::Private => text.literal("Privé"),
        InstanceSettingsMessage::PrivateText => text.literal(
            "Les nouveaux dépôts sont d’abord privés\u{202f}; leurs propriétaires peuvent toujours les rendre publics.",
        ),
        InstanceSettingsMessage::VisibilityHint => text.literal("Présélectionnée dans le formulaire de nouveau dépôt."),
        InstanceSettingsMessage::DigestTime => text.literal("Heure des récapitulatifs par e-mail"),
        InstanceSettingsMessage::DigestHint => text.literal(
            "L’heure d’envoi des récapitulatifs quotidiens et hebdomadaires de notifications aux utilisateurs qui n’en \
             ont pas choisi. Les récapitulatifs hebdomadaires partent le lundi.",
        ),
        InstanceSettingsMessage::Save => text.literal("Enregistrer les paramètres"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("Au moins ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " caractère.",
                _ => " caractères.",
            });
            text.literal(
                " Communiquez-le à l’utilisateur de manière sécurisée. Il devra le remplacer lors de sa première connexion.",
            );
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal(
                "Le nouveau mot de passe remplace immédiatement l’actuel et les liens de réinitialisation cessent de \
                 fonctionner. Communiquez-le à ",
            );
            text.argument(name);
            text.literal(" de manière sécurisée.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", ses dépôts (");
            text.argument(repositories);
            text.literal(
                ", y compris toutes les données Git), ses jetons d’accès et ses appartenances aux organisations. Cette \
                 action est irréversible.",
            );
        }
    }
}
