//! Japanese (`ja`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;

entries! { Ja:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("この実行は承認を待っています"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "この実行はフォーク、または書き込み権限のないユーザーからのものです。ワークフローがレビューされていないコードを実行する可能性があるため、書き込み権限を持つユーザーが先に承認する必要があります。承認後も、読み取り専用のトークンで実行され、シークレットや変数は使用できません。",
        ),
        RunPageMessage::TrustAuthor => {
            text.literal("この作成者のプルリクエストを常に承認なしで実行する");
        }
        RunPageMessage::ApproveAndRun => text.literal("承認して実行"),
        RunPageMessage::UntrustedNote => text.literal(
            "この実行はフォークまたは外部の作成者からのものです。読み取り専用のトークンで実行され、シークレットや変数は使用できません。",
        ),
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal(
                "この実行はフォークまたは外部の作成者からのものです。読み取り専用のトークンで実行され、シークレットや変数は使用できません。",
            );
            text.argument(approver);
            text.literal(" が承認しました。");
        }
        RunPageMessage::DeploymentWaiting => text.literal("レビュー待ち："),
        RunPageMessage::DeploymentJobs { jobs, count, environment } => {
            text.argument(jobs);
            text.literal(" は ");
            text.argument(environment);
            text.literal(" にデプロイします。先にこの環境のレビュアーの承認が必要です。");
        }
        RunPageMessage::Comment => text.literal("コメント"),
        RunPageMessage::Optional => text.literal("（任意）"),
        RunPageMessage::ApproveAndDeploy => text.literal("承認してデプロイ"),
        RunPageMessage::Reject => text.literal("却下"),
        RunPageMessage::Needs { jobs } => {
            text.literal("依存：");
            text.argument(jobs);
        }
    }
}
