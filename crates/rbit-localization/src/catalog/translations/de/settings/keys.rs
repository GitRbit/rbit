//! German (`de`) entries for `catalog/settings/keys.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { De:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("SSH-Schlüssel"),
        KeysMessage::IntroLead => {
            text.literal(
                "Mit Schlüsseln können Sie ohne Passwort über SSH klonen, abrufen und pushen. Fügen Sie den Inhalt \
                 Ihrer öffentlichen Schlüsseldatei ein, meist",
            );
        }
        KeysMessage::IntroTail => text.literal("."),
        KeysMessage::NewKey => text.literal("Neuer SSH-Schlüssel"),
        KeysMessage::TitleLabel => text.literal("Titel"),
        KeysMessage::Optional => text.literal("optional"),
        KeysMessage::TitlePlaceholder => text.literal("z. B. Arbeitslaptop"),
        KeysMessage::TitleHint => text.literal("Standardmäßig der Kommentar des Schlüssels."),
        KeysMessage::PublicKey => text.literal("Öffentlicher Schlüssel"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… you@example.com"),
        KeysMessage::KeyHintLead => {
            text.literal("Ed25519, ECDSA, RSA (2048 Bit oder mehr) oder ein Sicherheitsschlüssel (");
        }
        KeysMessage::KeyHintMiddle => text.literal("). Erstellen Sie einen mit"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("Schlüssel hinzufügen"),
        KeysMessage::YourKeys => text.literal("Ihre Schlüssel"),
        KeysMessage::EmptyTitle => text.literal("Keine SSH-Schlüssel"),
        KeysMessage::EmptyText => {
            text.literal("Fügen Sie einen öffentlichen Schlüssel hinzu, um Git über SSH zu verwenden.");
        }
        KeysMessage::Delete => text.literal("Löschen"),
        KeysMessage::TestLead => text.literal("Prüfen Sie, ob ein Schlüssel funktioniert, mit"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.argument(title);
            text.literal(" löschen");
        }
    }
}
