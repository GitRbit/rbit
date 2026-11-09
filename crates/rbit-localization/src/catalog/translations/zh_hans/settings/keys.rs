//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/keys.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { ZhHans:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("SSH 密钥"),
        KeysMessage::IntroLead => {
            text.literal("有了密钥，你无需密码即可通过 SSH 克隆、获取和推送。请粘贴公钥文件的内容，该文件通常是");
        }
        KeysMessage::IntroTail => text.literal("。"),
        KeysMessage::NewKey => text.literal("新建 SSH 密钥"),
        KeysMessage::TitleLabel => text.literal("标题"),
        KeysMessage::Optional => text.literal("可选"),
        KeysMessage::TitlePlaceholder => text.literal("例如：工作笔记本"),
        KeysMessage::TitleHint => text.literal("默认使用密钥的注释。"),
        KeysMessage::PublicKey => text.literal("公钥"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… you@example.com"),
        KeysMessage::KeyHintLead => text.literal("Ed25519、ECDSA、RSA（2048 位或以上）或安全密钥（"),
        KeysMessage::KeyHintMiddle => text.literal("）。可使用"),
        KeysMessage::KeyHintTail => text.literal("创建。"),
        KeysMessage::AddKey => text.literal("添加密钥"),
        KeysMessage::YourKeys => text.literal("你的密钥"),
        KeysMessage::EmptyTitle => text.literal("没有 SSH 密钥"),
        KeysMessage::EmptyText => text.literal("添加一个公钥即可通过 SSH 使用 Git。"),
        KeysMessage::Delete => text.literal("删除"),
        KeysMessage::TestLead => text.literal("可使用"),
        KeysMessage::TestTail => text.literal("检查密钥是否可用。"),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.literal("删除 ");
            text.argument(title);
        }
    }
}
