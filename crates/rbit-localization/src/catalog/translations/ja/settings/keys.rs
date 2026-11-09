//! Japanese (`ja`) entries for `catalog/settings/keys.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::KeysSentence;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Ja:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("SSH キー"),
        KeysMessage::NewKey => text.literal("新しい SSH キー"),
        KeysMessage::TitleLabel => text.literal("タイトル"),
        KeysMessage::Optional => text.literal("任意"),
        KeysMessage::TitlePlaceholder => text.literal("例：仕事用ノート PC"),
        KeysMessage::TitleHint => text.literal("省略するとキーのコメントが使われます。"),
        KeysMessage::PublicKey => text.literal("公開鍵"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… you@example.com"),
        KeysMessage::KeyHintLead => text.literal("Ed25519、ECDSA、RSA（2048 ビット以上）、またはセキュリティキー（"),
        KeysMessage::KeyHintMiddle => text.literal("）に対応しています。作成するには"),
        KeysMessage::KeyHintTail => text.literal("を実行します。"),
        KeysMessage::AddKey => text.literal("キーを追加"),
        KeysMessage::YourKeys => text.literal("登録済みのキー"),
        KeysMessage::EmptyTitle => text.literal("SSH キーはありません"),
        KeysMessage::EmptyText => text.literal("SSH 経由で Git を使うには、公開鍵を追加してください。"),
        KeysMessage::Delete => text.literal("削除"),
        KeysMessage::TestLead => text.literal("キーが機能するかどうかは"),
        KeysMessage::TestTail => text.literal("で確認できます。"),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.argument(title);
            text.literal(" を削除");
        }
    }
    KeysSentence => RichWriter |text| {
        KeysSentence::Intro => {
            text.literal("キーを登録すると、パスワードなしで SSH 経由のクローン、フェッチ、プッシュができます。公開鍵ファイル（通常は ");
            text.slot(0);
            text.literal("）の内容を貼り付けてください。");
        }
    }
}
