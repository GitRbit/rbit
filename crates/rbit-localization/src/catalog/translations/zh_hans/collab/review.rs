//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("更改"),
        ReviewMessage::ToggleFileTree => text.literal("显示或隐藏文件树"),
        ReviewMessage::CollapseAll => text.literal("全部折叠"),
        ReviewMessage::ExpandAll => text.literal("全部展开"),
        ReviewMessage::OutdatedComments { count } => {
            text.literal("对话中有 ");
            text.number(count);
            text.literal(" 条已过时的评论");
        }
        ReviewMessage::FinishReview => text.literal("完成审查"),
        ReviewMessage::Truncated => text.literal("此更改非常大，因此部分差异未显示。请克隆仓库以查看完整更改。"),
        ReviewMessage::FileToggleHint => text.literal("单击可折叠或展开 | 按住 Alt 单击可作用于所有文件"),
        ReviewMessage::ViewFile => text.literal("查看文件"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("评论第 ");
            text.number(line);
            text.literal(" 行");
        }
        ReviewMessage::StartReview => text.literal("开始审查"),
        ReviewMessage::AddSingleComment => text.literal("添加单条评论"),
        ReviewMessage::BinaryFile => text.literal("不显示二进制文件。"),
        ReviewMessage::TooLarge => text.literal("差异太大，无法显示。"),
        ReviewMessage::SubmoduleCommit => text.literal("子模块提交"),
        ReviewMessage::NoContentChanges => text.literal("内容没有更改。"),
        ReviewMessage::NoChangesTitle => text.literal("没有更改"),
        ReviewMessage::NoChangesText => text.literal("头部分支相对基础分支没有新增任何内容。"),
        ReviewMessage::Added => text.literal("已添加"),
        ReviewMessage::Deleted => text.literal("已删除"),
        ReviewMessage::Modified => text.literal("已修改"),
        ReviewMessage::Renamed => text.literal("已重命名"),
        ReviewMessage::Copied => text.literal("已复制"),
        ReviewMessage::Outdated => text.literal("已过时"),
        ReviewMessage::Pending => text.literal("待提交"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("已解决的对话 · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("回复…"),
        ReviewMessage::AddToReview => text.literal("添加到审查"),
        ReviewMessage::ReopenConversation => text.literal("重新打开对话"),
        ReviewMessage::ResolveConversation => text.literal("解决对话"),
        ReviewMessage::ReviewChanges => text.literal("审查更改"),
        ReviewMessage::PendingCount { count } => {
            text.number(count);
            text.literal(" 条待提交");
        }
        ReviewMessage::ReviewPlaceholder => text.literal("发表评论（支持 Markdown）"),
        ReviewMessage::RequestChanges => text.literal("请求更改"),
        ReviewMessage::Approve => text.literal("批准"),
        ReviewMessage::DiscardReview => text.literal("放弃待提交的审查"),
        ReviewMessage::DismissReview => text.literal("驳回审查"),
        ReviewMessage::DismissReason => text.literal("为什么驳回此审查？"),
        ReviewMessage::Dismiss => text.literal("驳回"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("已被 ");
            text.argument(by);
            text.literal(" 驳回：");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("审查者"),
        ReviewMessage::NoReviews => text.literal("还没有审查"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("移除对 ");
            text.argument(name);
            text.literal(" 的审查请求");
        }
        ReviewMessage::RemoveRequest => text.literal("移除请求"),
        ReviewMessage::RequestReviewFrom => text.literal("请求审查的用户"),
        ReviewMessage::Username => text.literal("用户名"),
        ReviewMessage::RequestReview => text.literal("请求审查"),
        ReviewMessage::Approved => text.literal("已批准"),
        ReviewMessage::ChangesRequested => text.literal("已请求更改"),
        ReviewMessage::Dismissed => text.literal("已驳回"),
        ReviewMessage::Commented => text.literal("已评论"),
        ReviewMessage::AwaitingReview => text.literal("等待审查"),
    }
}
