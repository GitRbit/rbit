//! Simplified Chinese (`zh-Hans`) entries for `catalog/run_page.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_page::RunPageMessage;

entries! { ZhHans:
    RunPageMessage<'a> => MessageWriter |text| {
        RunPageMessage::ApprovalTitle => text.literal("此运行正在等待批准"),
        RunPageMessage::ApprovalExplanation => text.literal(
            "它来自复刻，或来自没有写入权限的用户。其工作流可能会运行你尚未审查的代码，因此需要有写入权限的成员先批准。\
             即使获得批准，它也只能使用只读令牌运行，且无法使用机密或变量。",
        ),
        RunPageMessage::TrustAuthor => text.literal("始终无需批准即可运行此作者的拉取请求"),
        RunPageMessage::ApproveAndRun => text.literal("批准并运行"),
        RunPageMessage::UntrustedNote => {
            text.literal("此运行来自复刻或外部作者：它使用只读令牌，且没有机密或变量。");
        }
        RunPageMessage::UntrustedApprovedNote { approver } => {
            text.literal("此运行来自复刻或外部作者：它使用只读令牌，且没有机密或变量，已由 ");
            text.argument(approver);
            text.literal(" 批准。");
        }
        RunPageMessage::DeploymentWaiting => text.literal("等待审查："),
        RunPageMessage::DeploymentJobs { jobs, environment, .. } => {
            text.argument(jobs);
            text.literal(" 将部署到 ");
            text.argument(environment);
            text.literal("，需要先由该环境的审查者批准。");
        }
        RunPageMessage::Comment => text.literal("评论"),
        RunPageMessage::Optional => text.literal("（可选）"),
        RunPageMessage::ApproveAndDeploy => text.literal("批准并部署"),
        RunPageMessage::Reject => text.literal("拒绝"),
        RunPageMessage::Needs { jobs } => {
            text.literal("依赖 ");
            text.argument(jobs);
        }
    }
}
