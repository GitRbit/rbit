//! Simplified Chinese (`zh-Hans`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;

entries! { ZhHans:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("作业详情"),
        ActionsJobMessage::Steps => text.literal("步骤和日志"),
        ActionsJobMessage::RunsOn => text.literal("运行于"),
        ActionsJobMessage::Duration => text.literal("耗时"),
        ActionsJobMessage::RawLog => text.literal("原始日志"),
        ActionsJobMessage::Rerun => text.literal("重新运行作业"),
        ActionsJobMessage::NotStarted => text.literal("尚未开始"),
        ActionsJobMessage::NotStartedDescription => text.literal("运行器接手此作业后，步骤及其日志会显示在这里。"),
        ActionsJobMessage::RetainedWindow => {
            text.literal("步骤预览的长度有限。打开某个步骤可查看更多行，或下载原始日志。");
        }
        ActionsJobMessage::Expired => text.literal("此日志已按日志保留策略删除。"),
        ActionsJobMessage::NoOutput => text.literal("没有输出。"),
        ActionsJobMessage::EarlierLines => text.literal("已省略之前的行。"),
        ActionsJobMessage::ShowMore => text.literal("显示此步骤的更多内容"),
        ActionsJobMessage::Or => text.literal("，或"),
        ActionsJobMessage::DownloadAll => text.literal("下载原始日志以查看全部内容。"),
        ActionsJobMessage::Truncated => text.literal("作业写入的内容超出了实例的保留上限；之后的行已被丢弃。"),
        ActionsJobMessage::StepNotStarted => text.literal("此步骤尚未开始。"),
        ActionsJobMessage::Runner => text.literal("运行器"),
        ActionsJobMessage::QueuedAt => text.literal("入队"),
        ActionsJobMessage::StartedAt => text.literal("开始时间"),
        ActionsJobMessage::Attempt => text.literal("尝试"),
        ActionsJobMessage::FailedStep => text.literal("失败于"),
        ActionsJobMessage::LogTools => text.literal("日志工具"),
        ActionsJobMessage::SearchLogs => text.literal("搜索日志"),
        ActionsJobMessage::PreviousMatch => text.literal("上一个匹配项"),
        ActionsJobMessage::NextMatch => text.literal("下一个匹配项"),
        ActionsJobMessage::ShowTimestamps => text.literal("显示时间戳"),
        ActionsJobMessage::ExpandAll => text.literal("全部展开"),
        ActionsJobMessage::CollapseAll => text.literal("全部折叠"),
        ActionsJobMessage::DownloadLog => text.literal("下载日志"),
        ActionsJobMessage::LinkCopied => text.literal("已复制该行的链接"),
        ActionsJobMessage::NoMatches => text.literal("无匹配项"),
        ActionsJobMessage::SetUpJob => text.literal("设置作业"),
        ActionsJobMessage::CompleteJob => text.literal("完成作业"),
        ActionsJobMessage::Success => text.literal("成功"),
        ActionsJobMessage::Failure => text.literal("失败"),
        ActionsJobMessage::Cancelled => text.literal("已取消"),
        ActionsJobMessage::Skipped => text.literal("已跳过"),
        ActionsJobMessage::Queued => text.literal("排队中"),
        ActionsJobMessage::Running => text.literal("进行中"),
        ActionsJobMessage::Waiting => text.literal("等待中"),
        ActionsJobMessage::Approval => text.literal("等待批准"),
        ActionsJobMessage::WorkflowError => text.literal("工作流错误"),
        ActionsJobMessage::StepCount { count } => {
            text.number(count);
            text.literal(" 个步骤");
        }
    }
}
