//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("新建里程碑"),
        MilestoneMessage::EditMilestone => text.literal("编辑里程碑"),
        MilestoneMessage::EmptyTitle => text.literal("这里没有里程碑"),
        MilestoneMessage::EmptyText => text.literal("里程碑将面向某个发行版或日期的议题归为一组，并显示其进度。"),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("关闭于 ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("截止于 ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("已逾期，截止于 ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("无截止日期"),
        MilestoneMessage::Close => text.literal("关闭"),
        MilestoneMessage::Reopen => text.literal("重新打开"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("删除里程碑“");
            text.argument(title);
            text.literal("”？其中的议题会保留。");
        }
        MilestoneMessage::SearchIssues => text.literal("搜索这些议题"),
        MilestoneMessage::NothingHereTitle => text.literal("这里什么都没有"),
        MilestoneMessage::NothingHereText => text.literal("添加到此里程碑的议题和拉取请求会显示在这里。"),
        MilestoneMessage::FormIntro => text.literal("将面向某个发行版或日期的议题归为一组。"),
        MilestoneMessage::DueDate => text.literal("截止日期"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown，可选"),
        MilestoneMessage::CreateMilestone => text.literal("创建里程碑"),
        MilestoneMessage::SaveMilestone => text.literal("保存里程碑"),
        MilestoneMessage::Complete { percent } => {
            text.literal("已完成 ");
            text.number(percent);
            text.literal("%");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.number(count);
            text.literal(" 个开放");
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.number(count);
            text.literal(" 个已关闭");
        }
    }
}
