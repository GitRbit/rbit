//! Japanese (`ja`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;

entries! { Ja:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("ワークフロー"),
        ActionsOverviewMessage::AllWorkflows => text.literal("すべてのワークフロー"),
        ActionsOverviewMessage::AllRuns => text.literal("すべてのワークフロー実行"),
        ActionsOverviewMessage::Filters => text.literal("ワークフロー実行の絞り込み"),
        ActionsOverviewMessage::Branch => text.literal("ブランチ"),
        ActionsOverviewMessage::Actor => text.literal("実行者"),
        ActionsOverviewMessage::Event => text.literal("イベント"),
        ActionsOverviewMessage::Status => text.literal("ステータス"),
        ActionsOverviewMessage::AnyEvent => text.literal("すべてのイベント"),
        ActionsOverviewMessage::AnyStatus => text.literal("すべてのステータス"),
        ActionsOverviewMessage::Filter => text.literal("絞り込み"),
        ActionsOverviewMessage::Clear => text.literal("クリア"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("ワークフローを有効にする"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("ワークフローを無効にする"),
        ActionsOverviewMessage::WorkflowDisabled => {
            text.literal("このワークフローは無効です。有効にするまで、どのイベントでも実行は開始されません。");
        }
        ActionsOverviewMessage::NoMatchingRuns => text.literal("一致する実行はありません"),
        ActionsOverviewMessage::NoMatchingRunsDescription => {
            text.literal("これらの条件に一致するワークフロー実行はありません。");
        }
        ActionsOverviewMessage::NoRunsYet => text.literal("まだ実行はありません"),
        ActionsOverviewMessage::DispatchEmptyDescription => {
            text.literal("このワークフローは開始したときに実行されます。上の「ワークフローを実行」を使用してください。");
        }
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("まだワークフロー実行はありません"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal(
            ".rbit/workflows にワークフローファイルを追加してプッシュしてください。ランナーが受け取ると、実行がここに表示されます。",
        ),
        ActionsOverviewMessage::Pagination => text.literal("ページ送り"),
        ActionsOverviewMessage::OlderRuns => text.literal("以前の実行"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.literal("このページの実行：");
            text.number(count);
            text.literal(" 件");
        }
        ActionsOverviewMessage::JobsFinished { done, total } => {
            text.literal("完了したジョブ：");
            text.number(done);
            text.literal(" / ");
            text.number(total);
            text.literal(" 件");
        }
        ActionsOverviewMessage::JobsRunning { count } => {
            text.literal("実行中のジョブ：");
            text.number(count);
            text.literal(" 件");
        }
        ActionsOverviewMessage::WaitingForRunner => text.literal("ランナーを待機中"),
    }
}
