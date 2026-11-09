//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { ZhHans:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("通知"),
        InboxMessage::Show => text.literal("显示"),
        InboxMessage::Unread => text.literal("未读"),
        InboxMessage::All => text.literal("全部"),
        InboxMessage::MarkAllRead => text.literal("全部标为已读"),
        InboxMessage::NoNotifications => text.literal("没有通知"),
        InboxMessage::NoNotificationsText => {
            text.literal("你参与的议题和拉取请求会在这里通知你：包括你创建的、评论过的、分配给你的或提及你的。");
        }
        InboxMessage::AllCaughtUp => text.literal("全部看完了"),
        InboxMessage::AllCaughtUpText => text.literal("没有未读通知。较早的通知在“全部”中。"),
        InboxMessage::MarkAsRead => text.literal("标为已读"),
        InboxMessage::Unsubscribe => text.literal("取消订阅"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "开放的议题",
            ThreadState::ClosedIssue => "已关闭的议题",
            ThreadState::OpenPull => "开放的拉取请求",
            ThreadState::MergedPull => "已合并的拉取请求",
            ThreadState::ClosedPull => "已关闭的拉取请求",
        }),
        InboxMessage::StopEmails => text.literal("停止通知邮件"),
        InboxMessage::TurnOffFor(username) => {
            text.literal("要为 ");
            text.argument(username);
            text.literal(" 关闭通知邮件吗？通知仍会显示在收件箱中。");
        }
        InboxMessage::TurnOff => text.literal("关闭通知邮件"),
        InboxMessage::EmailsOff => text.literal("通知邮件已关闭"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(" 将不再收到通知邮件。你可以随时重新开启。");
        }
        InboxMessage::NotificationSettings => text.literal("通知设置"),
        InboxMessage::LinkNotValid => text.literal("此链接无效"),
        InboxMessage::LinkNotValidText => text.literal("链接可能不完整。请登录后选择要通过邮件接收哪些通知。"),
    }
}
