//! Japanese (`ja`) entries for `catalog/theme.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;
use crate::catalog::theme::ThemeSentence;

entries! { Ja:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("テーマを選択"),
        ThemeMessage::Search => text.literal("テーマを検索"),
        ThemeMessage::Close => text.literal("閉じる"),
        ThemeMessage::Show => text.literal("表示"),
        ThemeMessage::All => text.literal("すべて"),
        ThemeMessage::Dark => text.literal("ダーク"),
        ThemeMessage::Light => text.literal("ライト"),
        ThemeMessage::Colorblind => text.literal("色覚対応"),
        ThemeMessage::Families => text.literal("テーマファミリー"),
        ThemeMessage::Recent => text.literal("最近使用"),
        ThemeMessage::Themes => text.literal("テーマ"),
        ThemeMessage::Empty => text.literal("検索に一致するテーマはありません。"),
        ThemeMessage::Preview => text.literal("プレビュー"),
        ThemeMessage::System => text.literal("システム"),
        ThemeMessage::BuiltIn => text.literal("組み込み"),
        ThemeMessage::More => text.literal("その他のテーマ"),
        ThemeMessage::Auto => text.literal("自動"),
        ThemeMessage::Apply => text.literal("適用"),
        ThemeMessage::Applied => text.literal("適用済み"),
        ThemeMessage::By => text.literal("作成者"),
        ThemeMessage::Credit => text.literal("rbit に組み込み"),
        ThemeMessage::ThemeSingular => text.literal("テーマ"),
        ThemeMessage::ThemePlural => text.literal("テーマ"),
        ThemeMessage::OrApply => text.literal("または「適用」をクリック"),
    }
    ThemeSentence => RichWriter |text| {
        ThemeSentence::Count { .. } => {
            text.slot(0);
            text.literal(" 個のテーマ");
        }
    }
}
