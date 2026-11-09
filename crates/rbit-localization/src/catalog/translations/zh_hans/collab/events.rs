//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

/// `words`, then slot `slot`.
fn then(text: &mut RichWriter, words: &str, slot: usize) {
    text.literal(words);
    text.slot(slot);
}

/// How an issue or pull request was closed.
const fn reason(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => "将此关闭为已完成",
        CloseReason::NotPlanned => "将此关闭为不计划处理",
        CloseReason::Merged => "将此关闭为已合并",
    }
}

entries! { ZhHans:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason: why } => {
            then(text, "", 0);
            text.literal(" ");
            text.literal(reason(why));
        }
        TimelineEventMessage::ClosedIn { reason: why } => {
            then(text, "", 0);
            then(text, " 在 ", 1);
            text.literal(" 中");
            text.literal(reason(why));
        }
        TimelineEventMessage::Reopened => then_sentence(text, " 重新打开了此内容"),
        TimelineEventMessage::Renamed => {
            then(text, "", 0);
            then(text, " 将标题从 ", 1);
            then(text, " 改为 ", 2);
        }
        TimelineEventMessage::Locked => then_sentence(text, " 锁定了对话，并仅限协作者参与"),
        TimelineEventMessage::Unlocked => then_sentence(text, " 解锁了此对话"),
        TimelineEventMessage::Labeled => {
            then(text, "", 0);
            then(text, " 添加了标签 ", 1);
        }
        TimelineEventMessage::Unlabeled => {
            then(text, "", 0);
            then(text, " 移除了标签 ", 1);
        }
        TimelineEventMessage::Milestoned => {
            then(text, "", 0);
            then(text, " 将此添加到里程碑 ", 1);
        }
        TimelineEventMessage::Demilestoned => {
            then(text, "", 0);
            then(text, " 将此从里程碑 ", 1);
            text.literal(" 中移除");
        }
        TimelineEventMessage::SelfAssigned => then_sentence(text, " 将此分配给了自己"),
        TimelineEventMessage::Assigned => {
            then(text, "", 0);
            then(text, " 将此分配给了 ", 1);
        }
        TimelineEventMessage::SelfUnassigned => then_sentence(text, " 取消了自己的分配"),
        TimelineEventMessage::Unassigned => {
            then(text, "", 0);
            then(text, " 取消了对 ", 1);
            text.literal(" 的分配");
        }
        TimelineEventMessage::CommitReferenced => {
            then(text, "", 0);
            then(text, " 在提交 ", 1);
            text.literal(" 中引用了此内容");
        }
        TimelineEventMessage::MentionedInIssue => {
            then(text, "", 0);
            then(text, " 在议题 ", 1);
            text.literal(" 中提及了此内容");
        }
        TimelineEventMessage::MentionedInPull => {
            then(text, "", 0);
            then(text, " 在拉取请求 ", 1);
            text.literal(" 中提及了此内容");
        }
        TimelineEventMessage::Mentioned => then_sentence(text, " 提及了此内容"),
        TimelineEventMessage::ForcePushed => {
            then(text, "", 0);
            then(text, " 从 ", 2);
            then(text, " 强制推送到 ", 3);
        }
        TimelineEventMessage::ForcePushedBranch => {
            then(text, "", 0);
            then(text, " 将 ", 1);
            then(text, " 分支从 ", 2);
            then(text, " 强制推送到 ", 3);
        }
        TimelineEventMessage::Pushed { count } => {
            then(text, "", 0);
            text.literal(" 添加了 ");
            text.number(count);
            then(text, " 个提交，当前位于 ", 2);
        }
        TimelineEventMessage::PushedTo { count } => {
            then(text, "", 0);
            then(text, " 向 ", 1);
            text.literal(" 添加了 ");
            text.number(count);
            then(text, " 个提交，当前位于 ", 2);
        }
        TimelineEventMessage::BaseChanged => {
            then(text, "", 0);
            then(text, " 将基础分支从 ", 1);
            then(text, " 更改为 ", 2);
        }
        TimelineEventMessage::Merged => {
            then(text, "", 0);
            then(text, " 合并了提交 ", 1);
            text.slot(3);
        }
        TimelineEventMessage::MergedInto => {
            then(text, "", 0);
            then(text, " 将提交 ", 1);
            then(text, " 合并到 ", 2);
            text.slot(3);
        }
        TimelineEventMessage::ReviewRequestedSelf => then_sentence(text, " 请求自己进行审查"),
        TimelineEventMessage::ReviewRequested => {
            then(text, "", 0);
            then(text, " 请求 ", 1);
            text.literal(" 进行审查");
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            then(text, "", 0);
            then(text, " 移除了对 ", 1);
            text.literal(" 的审查请求");
        }
        TimelineEventMessage::ReviewDismissedOf => {
            then(text, "", 0);
            then(text, " 驳回了 ", 1);
            then(text, " 的", 2);
        }
        TimelineEventMessage::ReviewDismissed => {
            then(text, "", 0);
            then(text, " 驳回了一项", 2);
        }
        TimelineEventMessage::ThreadResolved => {
            then(text, "", 0);
            then(text, " 解决了", 1);
        }
        TimelineEventMessage::ThreadReopened => {
            then(text, "", 0);
            then(text, " 重新打开了", 1);
        }
        TimelineEventMessage::HeadRefDeleted => {
            then(text, "", 0);
            then(text, " 删除了 ", 1);
            text.literal(" 分支");
        }
        TimelineEventMessage::ConvertedToDraft => then_sentence(text, " 将此拉取请求标记为草稿"),
        TimelineEventMessage::ReadyForReview => then_sentence(text, " 将此拉取请求标记为可供审查"),
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("审查"),
        TimelineNoteMessage::ReviewConversation => text.literal("一个审查对话"),
        TimelineNoteMessage::MergeCommit => text.literal("（合并提交）"),
        TimelineNoteMessage::Squash => text.literal("（压缩合并）"),
        TimelineNoteMessage::Rebase => text.literal("（变基合并）"),
        TimelineNoteMessage::OutsideInstance => text.literal("（此实例之外）"),
    }
}

/// The actor, then `words`.
fn then_sentence(text: &mut RichWriter, words: &str) {
    text.slot(0);
    text.literal(words);
}
