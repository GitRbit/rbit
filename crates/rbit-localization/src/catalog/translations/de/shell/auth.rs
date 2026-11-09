//! German (`de`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};
use crate::plural::Plural;

entries! { De:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle => text.literal("Anmelden"),
        AuthMessage::SignUpTitle => text.literal("Registrieren"),
        AuthMessage::SignInTo(site) => {
            text.literal("Bei ");
            text.argument(site);
            text.literal(" anmelden");
        }
        AuthMessage::SignInLede => text.literal("Verwenden Sie Ihren Benutzernamen oder Ihre E-Mail-Adresse."),
        AuthMessage::UsernameOrEmail => text.literal("Benutzername oder E-Mail-Adresse"),
        AuthMessage::Password => text.literal("Passwort"),
        AuthMessage::ForgotPassword => text.literal("Passwort vergessen?"),
        AuthMessage::SignIn => text.literal("Anmelden"),
        AuthMessage::Or => text.literal("oder"),
        AuthMessage::SignInWith(provider) => {
            text.literal("Mit ");
            text.argument(provider);
            text.literal(" anmelden");
        }
        AuthMessage::NoAccount => text.literal("Noch kein Konto?"),
        AuthMessage::SignUp => text.literal("Registrieren"),
        AuthMessage::CreateYourAccount => text.literal("Konto erstellen"),
        AuthMessage::HostYourCode(site) => {
            text.literal("Hosten Sie Ihren Code auf ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::Username => text.literal("Benutzername"),
        AuthMessage::UsernameHint => text.literal("Buchstaben, Ziffern, „-“, „_“ und „.“"),
        AuthMessage::Email => text.literal("E-Mail-Adresse"),
        AuthMessage::MinimumLength(count) => {
            text.literal("Mindestens ");
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" Zeichen.");
        }
        AuthMessage::SetupToken => text.literal("Einrichtungstoken"),
        AuthMessage::SetupTokenHint => text.literal(
            "Dieses erste Konto verwaltet den Server. Das Token steht im Serverprotokoll.",
        ),
        AuthMessage::CreateAccount => text.literal("Konto erstellen"),
        AuthMessage::RegistrationClosed => text.literal(
            "Die Registrierung ist auf diesem Server geschlossen. Bitten Sie einen Administrator um ein Konto.",
        ),
        AuthMessage::HaveAccount => text.literal("Sie haben bereits ein Konto?"),
        AuthMessage::TwoFactorTitle => text.literal("Zwei-Faktor-Authentifizierung"),
        AuthMessage::PasskeyOrApp => text.literal(
            "Verwenden Sie Ihren Passkey oder geben Sie den 6-stelligen Code aus Ihrer Authenticator-App ein.",
        ),
        AuthMessage::PasskeyOnly => text.literal(
            "Verwenden Sie Ihren Passkey oder Sicherheitsschlüssel, um die Anmeldung abzuschließen.",
        ),
        AuthMessage::AppOnly => text.literal("Geben Sie den 6-stelligen Code aus Ihrer Authenticator-App ein."),
        AuthMessage::UsePasskey => text.literal("Passkey verwenden"),
        AuthMessage::PasskeyUnsupported => text.literal(
            "Dieser Browser kann keine Passkeys verwenden. Geben Sie stattdessen einen Code ein.",
        ),
        AuthMessage::AuthenticationCode => text.literal("Authentifizierungscode"),
        AuthMessage::LostDevice => text.literal(
            "Gerät verloren? Geben Sie stattdessen einen Ihrer Wiederherstellungscodes ein.",
        ),
        AuthMessage::RecoveryCode => text.literal("Wiederherstellungscode"),
        AuthMessage::LostPasskey => text.literal(
            "Passkey verloren? Geben Sie einen Ihrer Wiederherstellungscodes ein.",
        ),
        AuthMessage::VerifyCode => text.literal("Code bestätigen"),
        AuthMessage::NotYou => text.literal("Nicht Sie?"),
        AuthMessage::DifferentAccount => text.literal("Mit einem anderen Konto anmelden"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("Sie haben sich mit ");
            text.argument(provider);
            text.literal(" angemeldet. Wählen Sie einen Benutzernamen auf ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("Bestätigt durch ");
            text.argument(provider);
            text.literal(". Eine andere Adresse muss per E-Mail bestätigt werden.");
        }
        AuthMessage::ConfirmByEmail => text.literal(
            "Sie erhalten eine E-Mail, um diese Adresse zu bestätigen.",
        ),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("Sie melden sich mit ");
            text.argument(provider);
            text.literal(
                " an; Ihr Konto hat kein Passwort, bis Sie über einen Link zum Zurücksetzen eines festlegen. \
                 Für Git über HTTPS benötigen Sie ein persönliches Zugriffstoken.",
            );
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("und verknüpfen Sie ");
            text.argument(provider);
            text.literal(" in Ihren Sicherheitseinstellungen.");
        }
        AuthMessage::SignInFailed => text.literal("Die Anmeldung hat nicht funktioniert"),
        AuthMessage::BackToSignIn => text.literal("Zurück zur Anmeldung"),
        AuthMessage::BackToSecuritySettings => text.literal("Zurück zu den Sicherheitseinstellungen"),
        AuthMessage::IncorrectCredentials => text.literal("Benutzername oder Passwort ist falsch."),
        AuthMessage::Suspended => text.literal(
            "Dieses Konto wurde gesperrt. Wenden Sie sich an einen Administrator.",
        ),
        AuthMessage::SignInExpired => text.literal(
            "Ihre Anmeldung ist abgelaufen. Melden Sie sich erneut an.",
        ),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "Zu viele Anmeldeversuche. Versuchen Sie es in ",
                Attempt::SignUp => "Zu viele Registrierungsversuche. Versuchen Sie es in ",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal(match text.plural(seconds) {
                    Plural::One => " Sekunde erneut.",
                    _ => " Sekunden erneut.",
                });
            } else {
                let minutes = seconds.div_ceil(60);
                text.number(minutes);
                text.literal(match text.plural(minutes) {
                    Plural::One => " Minute erneut.",
                    _ => " Minuten erneut.",
                });
            }
        }
    }
}
