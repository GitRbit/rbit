//! Simplified Chinese (`zh-Hans`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;

entries! { ZhHans:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("工作流"),
        ActionsOverviewMessage::AllWorkflows => text.literal("所有工作流"),
        ActionsOverviewMessage::AllRuns => text.literal("所有工作流运行"),
        ActionsOverviewMessage::Filters => text.literal("工作流运行筛选条件"),
        ActionsOverviewMessage::Branch => text.literal("分支"),
        ActionsOverviewMessage::Actor => text.literal("执行者"),
        ActionsOverviewMessage::Event => text.literal("事件"),
        ActionsOverviewMessage::Status => text.literal("状态"),
        ActionsOverviewMessage::AnyEvent => text.literal("任意事件"),
        ActionsOverviewMessage::AnyStatus => text.literal("任意状态"),
        ActionsOverviewMessage::Filter => text.literal("筛选"),
        ActionsOverviewMessage::Clear => text.literal("清除"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("启用工作流"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("禁用工作流"),
        ActionsOverviewMessage::WorkflowDisabled => {
            text.literal("此工作流已禁用：在重新启用之前，任何事件都不会启动运行。");
        }
        ActionsOverviewMessage::NoMatchingRuns => text.literal("没有匹配的运行"),
        ActionsOverviewMessage::NoMatchingRunsDescription => text.literal("没有工作流运行符合这些筛选条件。"),
        ActionsOverviewMessage::NoRunsYet => text.literal("还没有运行"),
        ActionsOverviewMessage::DispatchEmptyDescription => {
            text.literal("此工作流在被启动时运行：请使用上方的“运行工作流”。");
        }
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("还没有工作流运行"),
        ActionsOverviewMessage::WorkflowEmptyDescription => {
            text.literal("在 .rbit/workflows 下添加工作流文件并推送：运行器接手后，运行会显示在这里。");
        }
        ActionsOverviewMessage::Pagination => text.literal("分页"),
        ActionsOverviewMessage::OlderRuns => text.literal("更早的运行"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.literal("本页有 ");
            text.number(count);
            text.literal(" 次运行");
        }
        ActionsOverviewMessage::JobsFinished { done, total } => {
            text.literal("已完成 ");
            text.number(done);
            text.literal(" / ");
            text.number(total);
            text.literal(" 个作业");
        }
        ActionsOverviewMessage::JobsRunning { count } => {
            text.literal("正在运行 ");
            text.number(count);
            text.literal(" 个作业");
        }
        ActionsOverviewMessage::WaitingForRunner => text.literal("正在等待运行器"),
    }
}
