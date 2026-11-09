//! Japanese (`ja`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { Ja:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("表示言語"),
        LanguageMessage::Label => text.literal("優先する言語"),
        LanguageMessage::BrowserDefault => text.literal("ブラウザー／デフォルト"),
        LanguageMessage::BrowserHelp => {
            text.literal("ブラウザーの対応言語に従います。利用できない場合は英語を使用します。");
        }
        LanguageMessage::Coverage => text.literal("まだ翻訳されていないテキストは英語で表示されます。"),
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("言語を保存"),
        LanguageMessage::Saved => text.literal("言語の設定を保存しました。"),
        LanguageMessage::Invalid => text.literal("一覧の言語、またはブラウザーのデフォルトを選択してください。"),
    }
}
