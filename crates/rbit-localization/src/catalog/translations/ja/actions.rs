//! Japanese (`ja`) entries for `catalog/actions.rs`.

use crate::catalog::actions::ActionsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ActionsMessage<'a> => MessageWriter |text| {
        ActionsMessage::Actions => text.literal("Actions"),
        ActionsMessage::Workflow => text.literal("ワークフロー"),
        ActionsMessage::Commit => text.literal("コミット"),
        ActionsMessage::Duration => text.literal("所要時間"),
        ActionsMessage::Attempt => text.literal("試行"),
        ActionsMessage::TriggeredBy => text.literal("実行者"),
        ActionsMessage::Jobs => text.literal("ジョブ"),
        ActionsMessage::Artifacts => text.literal("アーティファクト"),
        ActionsMessage::RunDetails => text.literal("実行の詳細"),
        ActionsMessage::CancelRun => text.literal("実行をキャンセル"),
        ActionsMessage::RerunFailed => text.literal("失敗したジョブを再実行"),
        ActionsMessage::RerunAll => text.literal("すべて再実行"),
        ActionsMessage::DownloadTar => text.literal("tar.gz でダウンロード"),
        ActionsMessage::Expires => text.literal("有効期限"),
        ActionsMessage::RerunJob { name } => {
            text.argument(name);
            text.literal(" を再実行");
        }
        ActionsMessage::Summary { job } => {
            text.argument(job);
            text.literal(" の概要");
        }
    }
}
