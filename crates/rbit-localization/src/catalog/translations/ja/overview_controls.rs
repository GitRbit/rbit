//! Japanese (`ja`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { Ja:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("プッシュ"),
        OverviewControlMessage::PullRequest => text.literal("プルリクエスト"),
        OverviewControlMessage::Manual => text.literal("手動"),
        OverviewControlMessage::Schedule => text.literal("スケジュール"),
        OverviewControlMessage::WorkflowRun => text.literal("ワークフローの完了後"),
        OverviewControlMessage::Dispatch => text.literal("ディスパッチ"),
        OverviewControlMessage::Success => text.literal("成功"),
        OverviewControlMessage::Failure => text.literal("失敗"),
        OverviewControlMessage::Cancelled => text.literal("キャンセル済み"),
        OverviewControlMessage::Skipped => text.literal("スキップ"),
        OverviewControlMessage::Queued => text.literal("キュー待ち"),
        OverviewControlMessage::Running => text.literal("実行中"),
        OverviewControlMessage::Waiting => text.literal("待機中"),
        OverviewControlMessage::Approval => text.literal("承認待ち"),
        OverviewControlMessage::RunWorkflow => text.literal("ワークフローを実行"),
        OverviewControlMessage::WorkflowFrom => text.literal("ワークフローの取得元"),
        OverviewControlMessage::Change => text.literal("変更"),
    }
}
