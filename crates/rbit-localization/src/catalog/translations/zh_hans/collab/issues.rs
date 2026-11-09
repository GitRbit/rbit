//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("议题"),
        IssueMessage::Labels => text.literal("标签"),
        IssueMessage::Milestones => text.literal("里程碑"),
        IssueMessage::NewIssue => text.literal("新建议题"),
        IssueMessage::SearchIssues => text.literal("搜索议题"),
        IssueMessage::SearchPlaceholder => text.literal("搜索标题和描述…"),
        IssueMessage::Search => text.literal("搜索"),
        IssueMessage::SearchHelp => text.literal("搜索帮助"),
        IssueMessage::ClearSearch => text.literal("清除搜索、筛选和排序"),
        IssueMessage::StateNav => text.literal("状态"),
        IssueMessage::NoIssuesTitle => text.literal("没有匹配的议题"),
        IssueMessage::NoIssuesText => text.literal("试试其他搜索，或创建第一个议题。"),
        IssueMessage::DescriptionHint => text.literal("支持 Markdown；#12 和 @name 会生成链接"),
        IssueMessage::NoLabelsYet => text.literal("还没有标签。"),
        IssueMessage::Assignees => text.literal("受理人"),
        IssueMessage::Milestone => text.literal("里程碑"),
        IssueMessage::NoMilestone => text.literal("无里程碑"),
        IssueMessage::OpenIssue => text.literal("创建议题"),
        IssueMessage::Details => text.literal("详情"),
        IssueMessage::Locked => text.literal("已锁定"),
        IssueMessage::AssignedTo { names } => {
            text.literal("已分配给 ");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(" 条评论");
        }
    }
}
