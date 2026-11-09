//! Simplified Chinese (`zh-Hans`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { ZhHans:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("推送"),
        OverviewControlMessage::PullRequest => text.literal("拉取请求"),
        OverviewControlMessage::Manual => text.literal("手动"),
        OverviewControlMessage::Schedule => text.literal("定时"),
        OverviewControlMessage::WorkflowRun => text.literal("在工作流之后"),
        OverviewControlMessage::Dispatch => text.literal("调度"),
        OverviewControlMessage::Success => text.literal("成功"),
        OverviewControlMessage::Failure => text.literal("失败"),
        OverviewControlMessage::Cancelled => text.literal("已取消"),
        OverviewControlMessage::Skipped => text.literal("已跳过"),
        OverviewControlMessage::Queued => text.literal("排队中"),
        OverviewControlMessage::Running => text.literal("进行中"),
        OverviewControlMessage::Waiting => text.literal("等待中"),
        OverviewControlMessage::Approval => text.literal("等待批准"),
        OverviewControlMessage::RunWorkflow => text.literal("运行工作流"),
        OverviewControlMessage::WorkflowFrom => text.literal("使用以下位置的工作流"),
        OverviewControlMessage::Change => text.literal("更改"),
    }
}
