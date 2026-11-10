//! Japanese (`ja`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;

entries! { Ja:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("ジョブの詳細"),
        ActionsJobMessage::Steps => text.literal("ステップとログ"),
        ActionsJobMessage::RunsOn => text.literal("実行環境"),
        ActionsJobMessage::Duration => text.literal("所要時間"),
        ActionsJobMessage::RawLog => text.literal("Raw ログ"),
        ActionsJobMessage::Rerun => text.literal("ジョブを再実行"),
        ActionsJobMessage::NotStarted => text.literal("まだ開始されていません"),
        ActionsJobMessage::NotStartedDescription => {
            text.literal("ランナーがこのジョブを受け取ると、ステップとそのログがここに表示されます。");
        }
        ActionsJobMessage::RetainedWindow => text.literal(
            "ステップのプレビューは一部のみです。さらに行を表示するにはステップを開くか、Raw ログをダウンロードしてください。",
        ),
        ActionsJobMessage::Expired => text.literal("このログはログの保持期間を過ぎたため削除されました。"),
        ActionsJobMessage::NoOutput => text.literal("出力はありません。"),
        ActionsJobMessage::EarlierLines => text.literal("それより前の行は省略されています。"),
        ActionsJobMessage::ShowMore => text.literal("このステップの続きを表示"),
        ActionsJobMessage::Or => text.literal("、または"),
        ActionsJobMessage::DownloadAll => text.literal("Raw ログをダウンロードしてすべて表示してください。"),
        ActionsJobMessage::Truncated => {
            text.literal("ジョブの出力がインスタンスの保持上限を超えたため、以降の行は破棄されました。");
        }
        ActionsJobMessage::StepNotStarted => text.literal("このステップはまだ開始されていません。"),
        ActionsJobMessage::SetUpJob => text.literal("ジョブを準備"),
        ActionsJobMessage::CompleteJob => text.literal("ジョブを完了"),
        ActionsJobMessage::Success => text.literal("成功"),
        ActionsJobMessage::Failure => text.literal("失敗"),
        ActionsJobMessage::Cancelled => text.literal("キャンセル済み"),
        ActionsJobMessage::Skipped => text.literal("スキップ"),
        ActionsJobMessage::Queued => text.literal("キュー待ち"),
        ActionsJobMessage::Running => text.literal("実行中"),
        ActionsJobMessage::Waiting => text.literal("待機中"),
        ActionsJobMessage::Approval => text.literal("承認待ち"),
        ActionsJobMessage::WorkflowError => text.literal("ワークフローエラー"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(" 個のステップ");
        }
    }
}
