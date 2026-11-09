//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("开放"),
        ThreadMessage::Closed => text.literal("已关闭"),
        ThreadMessage::Merged => text.literal("已合并"),
        ThreadMessage::Draft => text.literal("草稿"),
        ThreadMessage::Title => text.literal("标题"),
        ThreadMessage::Description => text.literal("描述"),
        ThreadMessage::Edit => text.literal("编辑"),
        ThreadMessage::Save => text.literal("保存"),
        ThreadMessage::Cancel => text.literal("取消"),
        ThreadMessage::Delete => text.literal("删除"),
        ThreadMessage::Optional => text.literal("可选"),
        ThreadMessage::MarkdownSupported => text.literal("支持 Markdown"),
        ThreadMessage::SignIn => text.literal("登录"),
        ThreadMessage::LoadMore => text.literal("加载更多"),
        ThreadMessage::AddComment => text.literal("添加评论"),
        ThreadMessage::CommentPlaceholder => text.literal("写下评论。使用 @ 提及用户，使用 # 引用议题或拉取请求。"),
        ThreadMessage::MentionSuggestions => text.literal("提及建议"),
        ThreadMessage::CommentHint => {
            text.literal("支持 Markdown。使用 @name、#123 或 owner/repo#123 链接用户、议题或拉取请求。");
        }
        ThreadMessage::LockedHint => text.literal("此对话已锁定；只有协作者可以评论。"),
        ThreadMessage::CloseIssue => text.literal("关闭议题"),
        ThreadMessage::ClosePull => text.literal("关闭拉取请求"),
        ThreadMessage::CloseNotPlanned => text.literal("以不计划处理关闭"),
        ThreadMessage::ReopenIssue => text.literal("重新打开议题"),
        ThreadMessage::ReopenPull => text.literal("重新打开拉取请求"),
        ThreadMessage::Comment => text.literal("评论"),
        ThreadMessage::LockConversation => text.literal("锁定对话"),
        ThreadMessage::UnlockConversation => text.literal("解锁对话"),
        ThreadMessage::Bot => text.literal("机器人"),
        ThreadMessage::Edited => text.literal("已编辑"),
        ThreadMessage::Author => text.literal("作者"),
        ThreadMessage::Reply => text.literal("回复"),
        ThreadMessage::DeleteCommentConfirm => text.literal("删除此评论？"),
        ThreadMessage::DeleteComment => text.literal("删除评论"),
        ThreadMessage::NoDescription => text.literal("未提供描述。"),
    }
}
