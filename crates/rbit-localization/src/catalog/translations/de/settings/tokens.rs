//! German (`de`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};
use crate::plural::Plural;

entries! { De:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("Zugriffstokens"),
        TokensMessage::IntroLead => text.literal(
            "Verwenden Sie für Git über HTTPS ein Token statt Ihres Passworts: Geben Sie Ihren Benutzernamen",
        ),
        TokensMessage::IntroUsername => text.literal(
            "ein und fügen Sie das Token ein, wenn Git nach einem Passwort fragt. Die REST-API unter",
        ),
        TokensMessage::IntroApi => text.literal("akzeptiert es als"),
        TokensMessage::IntroPrefix => text.literal(". Tokens beginnen mit"),
        TokensMessage::IntroLegacy => text.literal("; Tokens aus früheren Versionen beginnen mit"),
        TokensMessage::IntroTail => text.literal("und funktionieren weiterhin."),
        TokensMessage::CopyNow => text.literal(
            "Kopieren Sie Ihr neues Token jetzt. Sie können es später nicht mehr einsehen.",
        ),
        TokensMessage::CopyToken => text.literal("Token kopieren"),
        TokensMessage::Copy => text.literal("Kopieren"),
        TokensMessage::NewToken => text.literal("Neues Token"),
        TokensMessage::Name => text.literal("Name"),
        TokensMessage::NamePlaceholder => text.literal("z. B. Arbeitslaptop"),
        TokensMessage::NameHint => text.literal("Etwas, woran Sie es später wiedererkennen."),
        TokensMessage::Scope => text.literal("Bereich"),
        TokensMessage::ScopeHint => text.literal("Ein Token kann nie mehr als Ihr Konto."),
        TokensMessage::Expiration => text.literal("Ablauf"),
        TokensMessage::Generate => text.literal("Token generieren"),
        TokensMessage::YourTokens => text.literal("Ihre Tokens"),
        TokensMessage::EmptyTitle => text.literal("Keine Zugriffstokens"),
        TokensMessage::EmptyText => text.literal(
            "Generieren Sie ein Token, um ohne Ihr Passwort über HTTPS zu klonen, abzurufen und zu pushen.",
        ),
        TokensMessage::Revoke => text.literal("Widerrufen"),
        TokensMessage::NoExpiration => text.literal("Kein Ablauf"),
        TokensMessage::OneDay => text.literal("1 Tag"),
        TokensMessage::NeverUsed => text.literal("Nie verwendet"),
        TokensMessage::ReadDescription => text.literal("Klonen und abrufen; über die API lesen."),
        TokensMessage::WriteDescription => text.literal(
            "Zusätzlich pushen, Repositorys erstellen und über die API mit Stern markieren.",
        ),
        TokensMessage::AdminDescription => text.literal(
            "Zusätzlich über die API Repository-Einstellungen und Webhooks ändern und Repositorys löschen.",
        ),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("Token „");
            text.argument(name);
            text.literal("“ erstellt");
        }
        TokenText::RevokeLabel(name) => {
            text.argument(name);
            text.literal(" widerrufen");
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(match text.plural(days) {
                Plural::One => " Tag",
                _ => " Tage",
            });
        }
        TokenText::CreatedOn(date) => {
            text.literal("Erstellt am ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("Zuletzt verwendet ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("Läuft ab am ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("Abgelaufen am ");
            text.argument(date);
        }
    }
}
