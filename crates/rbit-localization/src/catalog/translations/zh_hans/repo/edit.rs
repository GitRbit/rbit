//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { ZhHans:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("新建文件"),
        RepoEditLabel::Edit => text.literal("编辑"),
        RepoEditLabel::On => text.literal("分支"),
        RepoEditLabel::NewFileHint => text.literal("在路径中输入 / 即可创建目录。"),
        RepoEditLabel::EditHint => text.literal("修改路径即可重命名或移动文件。"),
        RepoEditLabel::BackToFile => text.literal("返回文件"),
        RepoEditLabel::Path => text.literal("路径"),
        RepoEditLabel::Content => text.literal("内容"),
        RepoEditLabel::CommitChanges => text.literal("提交更改"),
        RepoEditLabel::Cancel => text.literal("取消"),
        RepoEditLabel::Delete => text.literal("删除"),
        RepoEditLabel::DeleteHint => text.literal("文件仍保留在历史记录中；变空的目录会消失。"),
        RepoEditLabel::DeleteFile => text.literal("删除文件"),
        RepoEditLabel::UploadFiles => text.literal("上传文件"),
        RepoEditLabel::Into => text.literal("上传到目录"),
        RepoEditLabel::OnBranch => text.literal("分支"),
        RepoEditLabel::UploadHint => text.literal("同名文件将被替换。"),
        RepoEditLabel::Files => text.literal("文件"),
        RepoEditLabel::NothingCommitted => text.literal("未提交任何内容。"),
        RepoEditLabel::ReviewNewCommits => text.literal("查看新提交"),
        RepoEditLabel::KeptAndCommittedOnTop => {
            text.literal("然后再次提交：你的更改保留在下方，并将在新版本之上提交。");
        }
        RepoEditLabel::BranchGone => text.literal("未提交任何内容。请改为提交到新分支；你的更改保留在下方。"),
        RepoEditLabel::CommitChangesLegend => text.literal("提交更改"),
        RepoEditLabel::CommitMessage => text.literal("提交信息"),
        RepoEditLabel::ExtendedDescription => text.literal("详细描述"),
        RepoEditLabel::Optional => text.literal("可选"),
        RepoEditLabel::CommitDirectlyTo => text.literal("直接提交到"),
        RepoEditLabel::NotAllowed => text.literal("不允许："),
        RepoEditLabel::NewBranchAndPull => text.literal("创建新分支并发起拉取请求"),
        RepoEditLabel::NewBranchName => text.literal("新分支名称"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("最大 ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("最多 ");
            text.number(u64::try_from(files).unwrap_or(u64::MAX));
            text.literal(" 个文件，每个文件不超过 ");
            text.argument(size);
            text.literal("，将一起提交。");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.literal("你开始编辑后，");
            text.argument(branch);
            text.literal(" 已发生变化");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" 已不存在");
        }
        RepoEditMessage::NowPointsAt => text.literal("它现在指向"),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("此文件是二进制文件或大于 ");
            text.argument(limit);
            text.literal("，无法在浏览器中编辑。你可以删除它，或上传一个替换文件。");
        }
        RepoEditMessage::EditingTitle { path } => {
            text.literal("正在编辑 ");
            text.argument(path);
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.literal("删除 ");
            text.argument(path);
        }
    }
}
