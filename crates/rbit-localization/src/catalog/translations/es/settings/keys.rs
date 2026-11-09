//! Spanish (`es`) entries for `catalog/settings/keys.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Es:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("Claves SSH"),
        KeysMessage::IntroLead => text.literal(
            "Las claves te permiten clonar, obtener y enviar por SSH sin contraseña. Pega el contenido de tu archivo \
             de clave pública, normalmente",
        ),
        KeysMessage::IntroTail => text.literal("."),
        KeysMessage::NewKey => text.literal("Nueva clave SSH"),
        KeysMessage::TitleLabel => text.literal("Título"),
        KeysMessage::Optional => text.literal("opcional"),
        KeysMessage::TitlePlaceholder => text.literal("p. ej., Portátil del trabajo"),
        KeysMessage::TitleHint => text.literal("Por defecto, el comentario de la clave."),
        KeysMessage::PublicKey => text.literal("Clave pública"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… tu@example.com"),
        KeysMessage::KeyHintLead => text.literal(
            "Ed25519, ECDSA, RSA (2048 bits o más) o una llave de seguridad (",
        ),
        KeysMessage::KeyHintMiddle => text.literal("). Crea una con"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("Añadir clave"),
        KeysMessage::YourKeys => text.literal("Tus claves"),
        KeysMessage::EmptyTitle => text.literal("No hay claves SSH"),
        KeysMessage::EmptyText => text.literal("Añade una clave pública para usar Git por SSH."),
        KeysMessage::Delete => text.literal("Eliminar"),
        KeysMessage::TestLead => text.literal("Comprueba que una clave funciona con"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.literal("Eliminar ");
            text.argument(title);
        }
    }
}
