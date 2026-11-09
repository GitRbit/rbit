//! Japanese (`ja`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::TokensSentence;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { Ja:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("アクセストークン"),
        TokensMessage::CopyNow => text.literal("新しいトークンを今すぐコピーしてください。再表示することはできません。"),
        TokensMessage::CopyToken => text.literal("トークンをコピー"),
        TokensMessage::Copy => text.literal("コピー"),
        TokensMessage::NewToken => text.literal("新しいトークン"),
        TokensMessage::Name => text.literal("名前"),
        TokensMessage::NamePlaceholder => text.literal("例：仕事用ノート PC"),
        TokensMessage::NameHint => text.literal("後で見分けられる名前を付けてください。"),
        TokensMessage::Scope => text.literal("スコープ"),
        TokensMessage::ScopeHint => text.literal("トークンにアカウント以上の権限が与えられることはありません。"),
        TokensMessage::Expiration => text.literal("有効期限"),
        TokensMessage::Generate => text.literal("トークンを生成"),
        TokensMessage::YourTokens => text.literal("登録済みのトークン"),
        TokensMessage::EmptyTitle => text.literal("アクセストークンはありません"),
        TokensMessage::EmptyText => text.literal("トークンを生成すると、パスワードなしで HTTPS 経由のクローン、フェッチ、プッシュができます。"),
        TokensMessage::Revoke => text.literal("取り消す"),
        TokensMessage::NoExpiration => text.literal("有効期限なし"),
        TokensMessage::OneDay => text.literal("1 日"),
        TokensMessage::NeverUsed => text.literal("未使用"),
        TokensMessage::ReadDescription => text.literal("クローンとフェッチ、API による読み取り。"),
        TokensMessage::WriteDescription => text.literal("加えて、プッシュ、および API によるリポジトリの作成とスター。"),
        TokensMessage::AdminDescription => text.literal("加えて、API によるリポジトリ設定と Webhook の変更、リポジトリの削除。"),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("トークン「");
            text.argument(name);
            text.literal("」を作成しました");
        }
        TokenText::RevokeLabel(name) => {
            text.argument(name);
            text.literal(" を取り消す");
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(" 日");
        }
        TokenText::CreatedOn(date) => {
            text.literal("作成：");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("最終使用：");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("有効期限：");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("期限切れ：");
            text.argument(date);
        }
    }
    TokensSentence => RichWriter |text| {
        TokensSentence::Intro => {
            text.literal("HTTPS 経由の Git では、パスワードの代わりにトークンを使います。ユーザー名 ");
            text.slot(0);
            text.literal(" を入力し、Git にパスワードを求められたらトークンを貼り付けてください。REST API（");
            text.slot(1);
            text.literal("）では、トークンを ");
            text.slot(2);
            text.literal(" として渡します。トークンは ");
            text.slot(3);
            text.literal(" で始まります。以前のリリースのトークンは ");
            text.slot(4);
            text.literal(" で始まり、引き続き使用できます。");
        }
    }
}
