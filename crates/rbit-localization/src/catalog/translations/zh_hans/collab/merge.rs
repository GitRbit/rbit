//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("合并"),
        MergeMessage::ConflictingFiles => text.literal("冲突的文件"),
        MergeMessage::WorkInProgress => text.literal("此拉取请求仍在进行中。"),
        MergeMessage::ReadyForReview => text.literal("可供审查"),
        MergeMessage::MergeMethod => text.literal("合并方式"),
        MergeMessage::CreateMergeCommit => text.literal("创建合并提交"),
        MergeMessage::SquashAndMerge => text.literal("压缩并合并"),
        MergeMessage::RebaseAndMerge => text.literal("变基并合并"),
        MergeMessage::EditMessage => text.literal("编辑提交信息"),
        MergeMessage::Message => text.literal("信息"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal("——压缩合并时为 ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("合并后删除头部分支"),
        MergeMessage::Bypass => text.literal("在不满足分支保护要求的情况下合并（此操作会被审计）"),
        MergeMessage::MergePull => text.literal("合并拉取请求"),
        MergeMessage::WriteAccessOnly => text.literal("只有对此仓库拥有写入权限的人才能合并。"),
        MergeMessage::PullOpen => text.literal("拉取请求处于开放状态"),
        MergeMessage::PullClosed => text.literal("拉取请求已关闭"),
        MergeMessage::StillDraft => text.literal("此拉取请求仍是草稿"),
        MergeMessage::HeadExists => text.literal("头部分支存在"),
        MergeMessage::HeadDeleted => text.literal("头部分支已被删除"),
        MergeMessage::NoConflicts => text.literal("与基础分支没有冲突"),
        MergeMessage::Conflicts => text.literal("此分支存在必须解决的冲突"),
        MergeMessage::CheckingConflicts => text.literal("正在检查冲突…"),
        MergeMessage::MergeabilityError => text.literal("无法检查是否可合并"),
        MergeMessage::Approvals { have, need } => {
            text.literal("已获得 ");
            text.number(have);
            text.literal(" 个批准，共需 ");
            text.number(need);
            text.literal(" 个");
        }
        MergeMessage::NoChangesRequested => text.literal("没有更改请求"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(" 位审查者请求了更改");
        }
        MergeMessage::MayPush => text.literal("你可以推送到基础分支"),
        MergeMessage::MayNotPush => text.literal("你不能推送到基础分支"),
        MergeMessage::NoChecks => text.literal("没有状态检查"),
        MergeMessage::ChecksPending => text.literal("部分检查尚未完成"),
        MergeMessage::ChecksPassed => text.literal("所有检查均已通过"),
        MergeMessage::ChecksFailed => text.literal("部分检查未成功"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(" 项必需检查已通过");
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.number(required);
            text.literal(" 项必需检查中有 ");
            text.number(failing);
            text.literal(" 项失败");
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("正在等待 ");
            text.number(required);
            text.literal(" 项必需检查中的 ");
            text.number(waiting);
            text.literal(" 项");
        }
    }
}
