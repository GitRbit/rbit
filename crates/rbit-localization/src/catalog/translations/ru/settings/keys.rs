//! Russian (`ru`) entries for `catalog/settings/keys.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::KeysSentence;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Ru:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("SSH-ключи"),
        KeysMessage::NewKey => text.literal("Новый SSH-ключ"),
        KeysMessage::TitleLabel => text.literal("Название"),
        KeysMessage::Optional => text.literal("необязательно"),
        KeysMessage::TitlePlaceholder => text.literal("например, Рабочий ноутбук"),
        KeysMessage::TitleHint => text.literal("По умолчанию — комментарий ключа."),
        KeysMessage::PublicKey => text.literal("Открытый ключ"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… you@example.com"),
        KeysMessage::KeyHintLead => text.literal("Ed25519, ECDSA, RSA (2048 бит и более) или ключ безопасности ("),
        KeysMessage::KeyHintMiddle => text.literal("). Создать ключ можно командой"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("Добавить ключ"),
        KeysMessage::YourKeys => text.literal("Ваши ключи"),
        KeysMessage::EmptyTitle => text.literal("SSH-ключей нет"),
        KeysMessage::EmptyText => text.literal("Добавьте открытый ключ, чтобы работать с Git по SSH."),
        KeysMessage::Delete => text.literal("Удалить"),
        KeysMessage::TestLead => text.literal("Проверить работу ключа можно командой"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.literal("Удалить ");
            text.argument(title);
        }
    }
    KeysSentence => RichWriter |text| {
        KeysSentence::Intro => {
            text.literal("Ключи позволяют клонировать, получать и отправлять изменения по SSH без пароля. Вставьте содержимое файла открытого ключа — обычно это ");
            text.slot(0);
            text.literal(".");
        }
    }
}
