//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("受理人"),
        IssueSidebarMessage::NoOne => text.literal("无"),
        IssueSidebarMessage::AssignYourself => text.literal("分配给自己"),
        IssueSidebarMessage::EditAssignees => text.literal("编辑受理人"),
        IssueSidebarMessage::SaveAssignees => text.literal("保存受理人"),
        IssueSidebarMessage::Labels => text.literal("标签"),
        IssueSidebarMessage::NoneYet => text.literal("暂无"),
        IssueSidebarMessage::EditLabels => text.literal("编辑标签"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("此仓库还没有标签。"),
        IssueSidebarMessage::SaveLabels => text.literal("保存标签"),
        IssueSidebarMessage::Milestone => text.literal("里程碑"),
        IssueSidebarMessage::NoMilestone => text.literal("无里程碑"),
        IssueSidebarMessage::ChangeMilestone => text.literal("更改里程碑"),
        IssueSidebarMessage::SaveMilestone => text.literal("保存里程碑"),
        IssueSidebarMessage::Notifications => text.literal("通知"),
        IssueSidebarMessage::Subscribe => text.literal("订阅"),
        IssueSidebarMessage::Unsubscribe => text.literal("取消订阅"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("你不会收到此议题的通知。"),
        IssueSidebarMessage::NotNotifiedPull => text.literal("你不会收到此拉取请求的通知。"),
        IssueSidebarMessage::Danger => text.literal("危险操作"),
        IssueSidebarMessage::DeleteIssue => text.literal("删除议题"),
        IssueSidebarMessage::DeletePull => text.literal("删除拉取请求"),
        IssueSidebarMessage::DeleteIssueConfirm => text.literal("要永久删除此议题吗？此操作无法撤销。"),
        IssueSidebarMessage::DeletePullConfirm => text.literal("要永久删除此拉取请求吗？此操作无法撤销。"),
        IssueSidebarMessage::ReasonManual => text.literal("你订阅了此讨论。"),
        IssueSidebarMessage::ReasonComment => text.literal("你评论过此讨论。"),
        IssueSidebarMessage::ReasonReviewed => text.literal("你审查过此拉取请求。"),
        IssueSidebarMessage::ReasonAuthor => text.literal("你创建了此讨论。"),
        IssueSidebarMessage::ReasonMention => text.literal("有人提及了你。"),
        IssueSidebarMessage::ReasonAssigned => text.literal("你是受理人。"),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("有人请求你进行审查。"),
    }
}
