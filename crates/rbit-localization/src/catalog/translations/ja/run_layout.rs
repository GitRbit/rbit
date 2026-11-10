//! Japanese (`ja`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { Ja:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("概要"),
        RunLayoutMessage::Navigation => text.literal("実行のナビゲーション"),
        RunLayoutMessage::Usage => text.literal("使用時間"),
        RunLayoutMessage::WorkflowFile => text.literal("ワークフローファイル"),
        RunLayoutMessage::JobCount { count } => {
            text.number(count);
            text.literal(" 件のジョブ");
        }
        RunLayoutMessage::Graph => text.literal("ジョブグラフ"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("他 ");
            text.number(count);
            text.literal(" 件");
        }
    }
}
