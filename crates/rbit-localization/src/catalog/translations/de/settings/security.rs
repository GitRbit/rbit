//! German (`de`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { De:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("Sicherheit"),
        SecurityMessage::Intro => text.literal(
            "Schützen Sie Ihr Konto mit einem Passkey oder einer Authenticator-App und behalten Sie im Blick, wo Sie angemeldet sind.",
        ),
        SecurityMessage::TwoFactor => text.literal("Zwei-Faktor-Authentifizierung"),
        SecurityMessage::On => text.literal("An"),
        SecurityMessage::Off => text.literal("Aus"),
        SecurityMessage::EnabledLead => text.literal(
            "Bei der Anmeldung wird nach Ihrem Passwort ein Passkey oder ein Code abgefragt. Für Git über HTTPS benötigen Sie ein",
        ),
        SecurityMessage::TokenLink => text.literal("persönliches Zugriffstoken"),
        SecurityMessage::EnabledTail => text.literal("; Ihr Passwort funktioniert dort nicht mehr."),
        SecurityMessage::Disabled => text.literal(
            "Fügen Sie der Anmeldung einen zweiten Schritt hinzu: einen Passkey, eine Authenticator-App oder beides.",
        ),
        SecurityMessage::Passkeys => text.literal("Passkeys und Sicherheitsschlüssel"),
        SecurityMessage::PasskeysText => text.literal(
            "Bestätigen Sie Anmeldungen mit Fingerabdruck, Gesicht, Geräte-PIN, Smartphone oder einem Hardwareschlüssel wie einem YubiKey.",
        ),
        SecurityMessage::Remove => text.literal("Entfernen"),
        SecurityMessage::OnlyFactor => text.literal(
            "Dies ist Ihr einziger zweiter Faktor: Wenn Sie ihn entfernen, wird die Zwei-Faktor-Authentifizierung deaktiviert und Ihre Wiederherstellungscodes werden gelöscht.",
        ),
        SecurityMessage::PasskeyGone => text.literal(
            "Sie können sich dann nicht mehr mit diesem Passkey anmelden.",
        ),
        SecurityMessage::Name => text.literal("Name"),
        SecurityMessage::PasskeyPlaceholder => text.literal("z. B. MacBook oder YubiKey"),
        SecurityMessage::AddPasskey => text.literal("Passkey hinzufügen"),
        SecurityMessage::PasskeyUnsupported => text.literal(
            "Zum Hinzufügen eines Passkeys benötigen Sie einen Browser, der Passkeys unterstützt, mit aktiviertem JavaScript.",
        ),
        SecurityMessage::PasskeyUrlLead => text.literal("Für Passkeys muss"),
        SecurityMessage::PasskeyUrlTail => text.literal("auf die Adresse dieser Website gesetzt sein."),
        SecurityMessage::Authenticator => text.literal("Authenticator-App"),
        SecurityMessage::AuthenticatorText => text.literal(
            "Verwenden Sie 6-stellige Codes aus einer App wie 1Password, Aegis oder Google Authenticator.",
        ),
        SecurityMessage::SetUp => text.literal("Einrichten"),
        SecurityMessage::RemoveAuthenticator => text.literal("Authenticator-App entfernen"),
        SecurityMessage::PasswordAlone => text.literal(
            "Ihr Konto ist dann nur noch durch Ihr Passwort geschützt, und Ihre Wiederherstellungscodes werden gelöscht.",
        ),
        SecurityMessage::PasskeysStay => text.literal("Ihre Passkeys bleiben als zweiter Faktor aktiv."),
        SecurityMessage::RecoveryCodes => text.literal("Wiederherstellungscodes"),
        SecurityMessage::Regenerate => text.literal("Neue Wiederherstellungscodes generieren"),
        SecurityMessage::CodesStop => text.literal(
            "Ihre aktuellen Wiederherstellungscodes funktionieren dann nicht mehr.",
        ),
        SecurityMessage::GenerateCodes => text.literal("Neue Codes generieren"),
        SecurityMessage::SignInMethods => text.literal("Anmeldemethoden"),
        SecurityMessage::Password => text.literal("Passwort"),
        SecurityMessage::NoPassword => text.literal("Kein Passwort"),
        SecurityMessage::WithPassword => text.literal(
            "Melden Sie sich mit Ihrem Passwort oder mit einem verknüpften Konto an.",
        ),
        SecurityMessage::LinkedOnlyLead => text.literal(
            "Ihr Konto hat kein Passwort: Sie melden sich mit einem verknüpften Konto an. Um eines festzulegen, fordern Sie einen",
        ),
        SecurityMessage::ResetLink => text.literal("Link zum Zurücksetzen des Passworts"),
        SecurityMessage::LinkedOnlyMiddle => text.literal(" an. Für Git über HTTPS benötigen Sie ein"),
        SecurityMessage::LinkedOnlyTail => text.literal("."),
        SecurityMessage::NoLongerAvailable => text.literal("Nicht mehr verfügbar"),
        SecurityMessage::Unlink => text.literal("Trennen"),
        SecurityMessage::OnlyWay => text.literal("Einzige Anmeldemöglichkeit"),
        SecurityMessage::NotLinked => text.literal("Nicht verknüpft"),
        SecurityMessage::Link => text.literal("Verknüpfen"),
        SecurityMessage::Sessions => text.literal("Sitzungen"),
        SecurityMessage::SignOutOthers => text.literal("Überall sonst abmelden"),
        SecurityMessage::ThisDevice => text.literal("Dieses Gerät"),
        SecurityMessage::SignOut => text.literal("Abmelden"),
        SecurityMessage::RecentActivity => text.literal("Letzte Aktivitäten"),
        SecurityMessage::NoActivity => text.literal("Noch keine Aktivitäten"),
        SecurityMessage::NoActivityText => text.literal(
            "Anmeldungen und Sicherheitsänderungen an Ihrem Konto werden hier angezeigt.",
        ),
        SecurityMessage::SetupTitle => text.literal("Authenticator-App einrichten"),
        SecurityMessage::SetupIntro => text.literal(
            "Scannen Sie den QR-Code mit Ihrer Authenticator-App und geben Sie dann den angezeigten 6-stelligen Code ein.",
        ),
        SecurityMessage::CantScan => text.literal(
            "Scannen nicht möglich? Geben Sie stattdessen diesen Schlüssel ein",
        ),
        SecurityMessage::CopyKey => text.literal("Schlüssel kopieren"),
        SecurityMessage::Copy => text.literal("Kopieren"),
        SecurityMessage::TimeBased => text.literal("Zeitbasiert, 6 Ziffern, alle 30 Sekunden."),
        SecurityMessage::ConfirmCode => text.literal("2. Code bestätigen"),
        SecurityMessage::AuthenticationCode => text.literal("Authentifizierungscode"),
        SecurityMessage::Cancel => text.literal("Abbrechen"),
        SecurityMessage::Enable => text.literal("Aktivieren"),
        SecurityMessage::TwoFactorOn => text.literal("Die Zwei-Faktor-Authentifizierung ist aktiviert."),
        SecurityMessage::SaveCodes => text.literal("Speichern Sie diese Codes jetzt"),
        SecurityMessage::CodesText => text.literal(
            "Mit jedem Code können Sie sich einmal anmelden, falls Sie Ihre Passkeys oder Ihre Authenticator-App verlieren. Sie werden nicht erneut angezeigt; bewahren Sie sie also sicher auf, etwa in einem Passwortmanager.",
        ),
        SecurityMessage::SavedCodes => text.literal("Ich habe meine Codes gespeichert"),
        SecurityMessage::AppOrRecoveryCode => text.literal(
            "Authentifizierungs- oder Wiederherstellungscode",
        ),
        SecurityMessage::RecoveryCode => text.literal("Wiederherstellungscode"),
        SecurityMessage::PasskeyInstead => text.literal(
            "Oder bestätigen Sie statt mit einem Code mit einem Ihrer Passkeys.",
        ),
        SecurityMessage::AuthenticatorRemoved => text.literal("Authenticator-App entfernt."),
        SecurityMessage::AuthenticatorAdded => text.literal("Authenticator-App hinzugefügt."),
        SecurityMessage::PasskeyAdded => text.literal("Passkey hinzugefügt."),
        SecurityMessage::PasskeyRemoved => text.literal("Passkey entfernt."),
        SecurityMessage::SessionRevoked => text.literal("Sitzung abgemeldet."),
        SecurityMessage::OtherSessionsRevoked => text.literal("Von allen anderen Sitzungen abgemeldet."),
        SecurityMessage::SetupCancelled => text.literal(
            "Einrichtung der Zwei-Faktor-Authentifizierung abgebrochen.",
        ),
        SecurityMessage::IdentityLinked => text.literal(
            "Konto verknüpft. Sie können sich jetzt damit anmelden.",
        ),
        SecurityMessage::IdentityUnlinked => text.literal("Kontoverknüpfung getrennt."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. ");
            text.argument(site);
            text.literal(" zu Ihrer App hinzufügen");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.argument(provider);
            text.literal(" trennen");
        }
        SecurityText::SignOutLabel(device) => {
            text.argument(device);
            text.literal(" abmelden");
        }
        SecurityText::WithPasskey(action) => {
            text.argument(action);
            text.literal(" mit einem Passkey");
        }
    }
}
