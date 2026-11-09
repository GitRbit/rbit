//! French (`fr`) entries for `catalog/settings/keys.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::KeysSentence;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Fr:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("Clés SSH"),
        KeysMessage::NewKey => text.literal("Nouvelle clé SSH"),
        KeysMessage::TitleLabel => text.literal("Titre"),
        KeysMessage::Optional => text.literal("facultatif"),
        KeysMessage::TitlePlaceholder => text.literal("p. ex. Ordinateur portable professionnel"),
        KeysMessage::TitleHint => text.literal("Par défaut, le commentaire de la clé."),
        KeysMessage::PublicKey => text.literal("Clé publique"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… vous@example.com"),
        KeysMessage::KeyHintLead => text.literal("Ed25519, ECDSA, RSA (2048\u{a0}bits ou plus) ou une clé de sécurité ("),
        KeysMessage::KeyHintMiddle => text.literal("). Créez-en une avec"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("Ajouter la clé"),
        KeysMessage::YourKeys => text.literal("Vos clés"),
        KeysMessage::EmptyTitle => text.literal("Aucune clé SSH"),
        KeysMessage::EmptyText => text.literal("Ajoutez une clé publique pour utiliser Git en SSH."),
        KeysMessage::Delete => text.literal("Supprimer"),
        KeysMessage::TestLead => text.literal("Vérifiez qu’une clé fonctionne avec"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.literal("Supprimer ");
            text.argument(title);
        }
    }
    KeysSentence => RichWriter |text| {
        KeysSentence::Intro => {
            text.literal("Les clés vous permettent de cloner, récupérer et pousser en SSH sans mot de passe. Collez le contenu de votre fichier de clé publique, généralement ");
            text.slot(0);
            text.literal(".");
        }
    }
}
