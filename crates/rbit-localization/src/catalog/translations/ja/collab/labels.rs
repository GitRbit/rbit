//! Japanese (`ja`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("新しいラベル"),
        LabelMessage::Name => text.literal("名前"),
        LabelMessage::Colour => text.literal("色"),
        LabelMessage::CreateLabel => text.literal("ラベルを作成"),
        LabelMessage::Count { count } => {
            text.number(count);
            text.literal(" 個のラベル");
        }
        LabelMessage::EmptyTitle => text.literal("ラベルはまだありません"),
        LabelMessage::EmptyText => text.literal("ラベルで Issue やプルリクエストを分類できます：bug、enhancement、good first issue…"),
        LabelMessage::CreateDefaults => text.literal("デフォルトのラベルを作成"),
        LabelMessage::OpenIssues { count } => {
            text.literal("オープンな Issue ");
            text.number(count);
            text.literal(" 件");
        }
        LabelMessage::DeleteConfirm { name } => {
            text.literal("ラベル「");
            text.argument(name);
            text.literal("」を削除しますか？すべての Issue から外されます。");
        }
    }
}
