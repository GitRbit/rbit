//! French (`fr`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};
use crate::plural::Plural;

entries! { Fr:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle | AuthMessage::SignIn => text.literal("Se connecter"),
        AuthMessage::SignUpTitle | AuthMessage::SignUp => text.literal("S’inscrire"),
        AuthMessage::SignInTo(site) => {
            text.literal("Se connecter à ");
            text.argument(site);
        }
        AuthMessage::SignInLede => text.literal("Utilisez votre nom d’utilisateur ou votre adresse e-mail."),
        AuthMessage::UsernameOrEmail => text.literal("Nom d’utilisateur ou adresse e-mail"),
        AuthMessage::Password => text.literal("Mot de passe"),
        AuthMessage::ForgotPassword => text.literal("Mot de passe oublié\u{202f}?"),
        AuthMessage::Or => text.literal("ou"),
        AuthMessage::SignInWith(provider) => {
            text.literal("Se connecter avec ");
            text.argument(provider);
        }
        AuthMessage::NoAccount => text.literal("Vous n’avez pas de compte\u{202f}?"),
        AuthMessage::CreateYourAccount => text.literal("Créer votre compte"),
        AuthMessage::HostYourCode(site) => {
            text.literal("Hébergez votre code sur ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::Username => text.literal("Nom d’utilisateur"),
        AuthMessage::UsernameHint => text.literal("Lettres, chiffres, «\u{a0}-\u{a0}», «\u{a0}_\u{a0}» et «\u{a0}.\u{a0}»"),
        AuthMessage::Email => text.literal("Adresse e-mail"),
        AuthMessage::MinimumLength(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.literal("Au moins ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " caractère.",
                _ => " caractères.",
            });
        }
        AuthMessage::SetupToken => text.literal("Jeton d’installation"),
        AuthMessage::SetupTokenHint => text.literal("Ce premier compte administre le serveur. Le jeton figure dans le journal du serveur."),
        AuthMessage::CreateAccount => text.literal("Créer le compte"),
        AuthMessage::RegistrationClosed => text.literal("Les inscriptions sont fermées sur ce serveur. Demandez un compte à un administrateur."),
        AuthMessage::HaveAccount => text.literal("Vous avez déjà un compte\u{202f}?"),
        AuthMessage::TwoFactorTitle => text.literal("Authentification à deux facteurs"),
        AuthMessage::PasskeyOrApp => text.literal("Utilisez votre clé d’accès ou saisissez le code à 6\u{a0}chiffres de votre application d’authentification."),
        AuthMessage::PasskeyOnly => text.literal("Utilisez votre clé d’accès ou votre clé de sécurité pour terminer la connexion."),
        AuthMessage::AppOnly => text.literal("Saisissez le code à 6\u{a0}chiffres de votre application d’authentification."),
        AuthMessage::UsePasskey => text.literal("Utiliser une clé d’accès"),
        AuthMessage::PasskeyUnsupported => text.literal("Ce navigateur ne prend pas en charge les clés d’accès. Saisissez plutôt un code."),
        AuthMessage::AuthenticationCode => text.literal("Code d’authentification"),
        AuthMessage::LostDevice => text.literal("Appareil perdu\u{202f}? Saisissez plutôt l’un de vos codes de récupération."),
        AuthMessage::RecoveryCode => text.literal("Code de récupération"),
        AuthMessage::LostPasskey => text.literal("Clé d’accès perdue\u{202f}? Saisissez l’un de vos codes de récupération."),
        AuthMessage::VerifyCode => text.literal("Vérifier le code"),
        AuthMessage::NotYou => text.literal("Ce n’est pas vous\u{202f}?"),
        AuthMessage::DifferentAccount => text.literal("Se connecter avec un autre compte"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("Vous vous êtes connecté avec ");
            text.argument(provider);
            text.literal(". Choisissez un nom d’utilisateur sur ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("Vérifiée par ");
            text.argument(provider);
            text.literal(". Une autre adresse doit être confirmée par e-mail.");
        }
        AuthMessage::ConfirmByEmail => text.literal("Vous recevrez un e-mail pour confirmer cette adresse."),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("Vous vous connecterez avec ");
            text.argument(provider);
            text.literal("\u{202f}; votre compte n’a pas de mot de passe tant que vous n’en définissez pas un avec un lien de réinitialisation. Git en HTTPS nécessite un jeton d’accès personnel.");
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("et associez ");
            text.argument(provider);
            text.literal(" dans vos paramètres de sécurité.");
        }
        AuthMessage::SignInFailed => text.literal("La connexion a échoué"),
        AuthMessage::BackToSignIn => text.literal("Retour à la connexion"),
        AuthMessage::BackToSecuritySettings => text.literal("Retour aux paramètres de sécurité"),
        AuthMessage::IncorrectCredentials => text.literal("Nom d’utilisateur ou mot de passe incorrect."),
        AuthMessage::Suspended => text.literal("Ce compte a été suspendu. Contactez un administrateur."),
        AuthMessage::SignInExpired => text.literal("Votre session de connexion a expiré. Reconnectez-vous."),
        AuthMessage::TooManyAttempts(attempt, seconds) => too_many_attempts(text, attempt, seconds),
    }
}

/// « Trop de tentatives de connexion. Réessayez dans 2 minutes. »
fn too_many_attempts(text: &mut MessageWriter, attempt: Attempt, seconds: u64) {
    text.literal(match attempt {
        Attempt::SignIn => "Trop de tentatives de connexion. Réessayez dans ",
        Attempt::SignUp => "Trop de tentatives d’inscription. Réessayez dans ",
    });
    let (count, one, other) =
        if seconds < 60 { (seconds, " seconde.", " secondes.") } else { (seconds.div_ceil(60), " minute.", " minutes.") };
    text.number(count);
    text.literal(match text.plural(count) {
        Plural::One => one,
        _ => other,
    });
}
