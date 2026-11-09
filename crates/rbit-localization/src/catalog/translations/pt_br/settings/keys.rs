//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/keys.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::KeysSentence;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { PtBr:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("Chaves SSH"),
        KeysMessage::NewKey => text.literal("Nova chave SSH"),
        KeysMessage::TitleLabel => text.literal("Título"),
        KeysMessage::Optional => text.literal("opcional"),
        KeysMessage::TitlePlaceholder => text.literal("ex.: Notebook do trabalho"),
        KeysMessage::TitleHint => text.literal("Por padrão, o comentário da chave."),
        KeysMessage::PublicKey => text.literal("Chave pública"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… voce@example.com"),
        KeysMessage::KeyHintLead => {
            text.literal("Ed25519, ECDSA, RSA (2048 bits ou mais) ou uma chave de segurança (");
        }
        KeysMessage::KeyHintMiddle => text.literal("). Crie uma com"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("Adicionar chave"),
        KeysMessage::YourKeys => text.literal("Suas chaves"),
        KeysMessage::EmptyTitle => text.literal("Nenhuma chave SSH"),
        KeysMessage::EmptyText => text.literal("Adicione uma chave pública para usar o Git por SSH."),
        KeysMessage::Delete => text.literal("Excluir"),
        KeysMessage::TestLead => text.literal("Verifique se uma chave funciona com"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.literal("Excluir ");
            text.argument(title);
        }
    }
    KeysSentence => RichWriter |text| {
        KeysSentence::Intro => {
            text.literal("As chaves permitem clonar, buscar e fazer push por SSH sem senha. Cole o conteúdo do arquivo da sua chave pública, geralmente ");
            text.slot(0);
            text.literal(".");
        }
    }
}
