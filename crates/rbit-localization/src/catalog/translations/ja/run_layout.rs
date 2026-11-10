//! Japanese (`ja`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { Ja:
    RunLayoutMessage<'a> => MessageWriter |text| {
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
        RunLayoutMessage::Annotations => text.literal("アノテーション"),
        RunLayoutMessage::Errors { count } => {
            text.literal("エラー ");
            text.number(count);
            text.literal(" 件");
        }
        RunLayoutMessage::Warnings { count } => {
            text.literal("警告 ");
            text.number(count);
            text.literal(" 件");
        }
        RunLayoutMessage::Notices { count } => {
            text.literal("通知 ");
            text.number(count);
            text.literal(" 件");
        }
        RunLayoutMessage::ErrorLevel => text.literal("エラー"),
        RunLayoutMessage::WarningLevel => text.literal("警告"),
        RunLayoutMessage::NoticeLevel => text.literal("通知"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("ステップ ");
            text.argument(step);
            text.literal(" が失敗しました。");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("ほか ");
            text.number(count);
            text.literal(" 件はジョブのログにあります。");
        }
        RunLayoutMessage::TriggeredVia => text.literal("トリガー"),
        RunLayoutMessage::Status => text.literal("状態"),
        RunLayoutMessage::TotalDuration => text.literal("合計時間"),
        RunLayoutMessage::RerunBy { name } => {
            text.argument(name);
            text.literal(" が再実行");
        }
        RunLayoutMessage::Inputs => text.literal("入力"),
        RunLayoutMessage::Waited { time } => {
            text.literal("待機 ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("ランナー ");
            text.argument(runner);
        }
    }
}
