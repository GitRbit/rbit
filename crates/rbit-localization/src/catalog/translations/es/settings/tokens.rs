//! Spanish (`es`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::TokensSentence;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};
use crate::plural::Plural;

entries! { Es:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("Tokens de acceso"),
        TokensMessage::CopyNow => text.literal("Copia ahora tu nuevo token. No podrás volver a verlo."),
        TokensMessage::CopyToken => text.literal("Copiar token"),
        TokensMessage::Copy => text.literal("Copiar"),
        TokensMessage::NewToken => text.literal("Nuevo token"),
        TokensMessage::Name => text.literal("Nombre"),
        TokensMessage::NamePlaceholder => text.literal("p. ej., Portátil del trabajo"),
        TokensMessage::NameHint => text.literal("Algo para reconocerlo más adelante."),
        TokensMessage::Scope => text.literal("Alcance"),
        TokensMessage::ScopeHint => text.literal("Un token nunca puede hacer más que tu cuenta."),
        TokensMessage::Expiration => text.literal("Caducidad"),
        TokensMessage::Generate => text.literal("Generar token"),
        TokensMessage::YourTokens => text.literal("Tus tokens"),
        TokensMessage::EmptyTitle => text.literal("No hay tokens de acceso"),
        TokensMessage::EmptyText => text.literal(
            "Genera un token para clonar, obtener y enviar por HTTPS sin tu contraseña.",
        ),
        TokensMessage::Revoke => text.literal("Revocar"),
        TokensMessage::NoExpiration => text.literal("Sin caducidad"),
        TokensMessage::OneDay => text.literal("1 día"),
        TokensMessage::NeverUsed => text.literal("Nunca usado"),
        TokensMessage::ReadDescription => text.literal("Clonar y obtener; leer mediante la API."),
        TokensMessage::WriteDescription => text.literal(
            "También enviar, crear repositorios y marcar con estrella mediante la API.",
        ),
        TokensMessage::AdminDescription => text.literal(
            "También cambiar la configuración y los webhooks de los repositorios, y eliminar repositorios mediante \
             la API.",
        ),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("Token «");
            text.argument(name);
            text.literal("» creado");
        }
        TokenText::RevokeLabel(name) => {
            text.literal("Revocar ");
            text.argument(name);
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(match text.plural(days) {
                Plural::One => " día",
                _ => " días",
            });
        }
        TokenText::CreatedOn(date) => {
            text.literal("Creado ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("Último uso ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("Caduca ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("Caducó ");
            text.argument(date);
        }
    }
    TokensSentence => RichWriter |text| {
        TokensSentence::Intro => {
            text.literal("Usa un token en lugar de tu contraseña para Git por HTTPS: introduce tu nombre de usuario ");
            text.slot(0);
            text.literal(" y pega el token cuando Git te pida una contraseña. La API REST de ");
            text.slot(1);
            text.literal(" lo acepta como ");
            text.slot(2);
            text.literal(". Los tokens empiezan por ");
            text.slot(3);
            text.literal("; los tokens de versiones anteriores empiezan por ");
            text.slot(4);
            text.literal(" y siguen funcionando.");
        }
    }
}
