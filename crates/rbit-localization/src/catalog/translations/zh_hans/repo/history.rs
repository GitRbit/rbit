//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};

/// A count as the English arm writes it.
fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

entries! { ZhHans:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("浏览此提交时的文件"),
        RepoHistoryLabel::BrowseFiles => text.literal("浏览文件"),
        RepoHistoryLabel::Pagination => text.literal("分页"),
        RepoHistoryLabel::Newer => text.literal("较新"),
        RepoHistoryLabel::Older => text.literal("较早"),
        RepoHistoryLabel::Committed => text.literal("提交于"),
        RepoHistoryLabel::CommittedBy => text.literal("提交者"),
        RepoHistoryLabel::Commit => text.literal("提交"),
        RepoHistoryLabel::Checks => text.literal("检查"),
        RepoHistoryLabel::ViewFile => text.literal("查看文件"),
        RepoHistoryLabel::PathInThatCommit => text.literal("该提交中的路径"),
        RepoHistoryLabel::FileEmpty => text.literal("此文件为空。"),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("无法追溯二进制文件。"),
        RepoHistoryLabel::TooLargeToBlame => text.literal("此文件太大，无法追溯。"),
        RepoHistoryLabel::ViewTheFile => text.literal("查看文件"),
        RepoHistoryLabel::CompareChanges => text.literal("比较更改"),
        RepoHistoryLabel::CompareLead => {
            text.literal("选择一个基础，以及要与之比较的分支、标记或提交，也可以跨复刻比较。");
        }
        RepoHistoryLabel::Comparison => text.literal("比较"),
        RepoHistoryLabel::MergeBase => text.literal("合并基础"),
        RepoHistoryLabel::Swap => text.literal("交换"),
        RepoHistoryLabel::CreatePullRequest => text.literal("创建拉取请求"),
        RepoHistoryLabel::UnrelatedHistories => text.literal("这些修订的历史记录完全不同，因此没有可显示的差异。"),
        RepoHistoryLabel::NothingToCompare => text.literal("没有可比较的内容"),
        RepoHistoryLabel::NothingToCompareText => text.literal("所比较修订中的所有内容都已在基础中。"),
        RepoHistoryLabel::Commits => text.literal("提交"),
        RepoHistoryLabel::Compare => text.literal("比较"),
        RepoHistoryLabel::CompareWithDefault => text.literal("与默认分支比较"),
        RepoHistoryLabel::BranchView => text.literal("分支视图"),
        RepoHistoryLabel::List => text.literal("列表"),
        RepoHistoryLabel::GitTree => text.literal("Git 树"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("没有匹配的引用。"),
        RepoHistoryLabel::NoBranchesYet => text.literal("还没有分支"),
        RepoHistoryLabel::NoBranchesText => text.literal("推送一个提交即可创建第一个分支。"),
        RepoHistoryLabel::NoTagsYet => text.literal("还没有标记"),
        RepoHistoryLabel::NoTagsText => text.literal("标记用于标识发行版和历史记录中的其他重要节点。"),
        RepoHistoryLabel::Protected => text.literal("受保护"),
        RepoHistoryLabel::ProtectedTitle => text.literal("此分支适用分支保护规则"),
        RepoHistoryLabel::DownloadTarGz => text.literal("下载 tar.gz"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("分支提交图"),
        RepoHistoryLabel::GraphHelp => text.literal("沿着彩色线条追踪历史记录。选择一条轨道可高亮其提交。"),
        RepoHistoryLabel::ClearHighlight => text.literal("清除高亮"),
        RepoHistoryLabel::HistoryConnections => text.literal("历史连接"),
        RepoHistoryLabel::CommitDetails => text.literal("提交详情"),
        RepoHistoryLabel::Merge => text.literal("合并"),
        RepoHistoryLabel::GraphTruncated => text.literal(
            "显示所有分支中最新的 200 个提交。底部的线条延续到更早的历史记录。打开某个分支的历史记录可继续浏览。",
        ),
        RepoHistoryLabel::LinesAdded => text.literal("新增"),
        RepoHistoryLabel::LinesDeleted => text.literal("删除"),
        RepoHistoryLabel::StatusAdded => text.literal("已添加"),
        RepoHistoryLabel::StatusDeleted => text.literal("已删除"),
        RepoHistoryLabel::StatusModified => text.literal("已修改"),
        RepoHistoryLabel::StatusRenamed => text.literal("已重命名"),
        RepoHistoryLabel::StatusCopied => text.literal("已复制"),
        RepoHistoryLabel::ChangedFiles => text.literal("更改的文件"),
        RepoHistoryLabel::FileTree => text.literal("文件树"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("筛选更改的文件"),
        RepoHistoryLabel::FilterFiles => text.literal("筛选文件…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("没有匹配的文件。"),
        RepoHistoryLabel::Changes => text.literal("更改"),
        RepoHistoryLabel::ToggleFileTree => text.literal("显示或隐藏文件树"),
        RepoHistoryLabel::CollapseAll => text.literal("全部折叠"),
        RepoHistoryLabel::ExpandAll => text.literal("全部展开"),
        RepoHistoryLabel::DiffTruncated => {
            text.literal("此更改非常大，因此部分差异未显示。请克隆仓库以查看完整更改。");
        }
        RepoHistoryLabel::CollapseHint => text.literal("单击可折叠或展开 · 按住 Alt 单击可作用于所有文件"),
        RepoHistoryLabel::CopyPath => text.literal("复制路径"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("查看此提交时的文件"),
        RepoHistoryLabel::View => text.literal("查看"),
        RepoHistoryLabel::BinaryNotShown => text.literal("不显示二进制文件。"),
        RepoHistoryLabel::DiffTooLarge => text.literal("差异太大，无法显示。"),
        RepoHistoryLabel::SubmoduleCommit => text.literal("子模块提交"),
        RepoHistoryLabel::NoContentChanges => text.literal("内容没有更改。"),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date } => {
            text.argument(date);
            text.literal(" 的提交");
        }
        RepoHistoryMessage::ViewCommit { id } => {
            text.literal("查看提交 ");
            text.argument(id);
        }
        RepoHistoryMessage::ParentNoun { .. } => text.literal("父提交"),
        RepoHistoryMessage::BlameOf { file } => {
            text.argument(file);
            text.literal(" 的追溯");
        }
        RepoHistoryMessage::BlameTitle { path } => {
            text.literal("追溯 ");
            text.argument(path);
        }
        RepoHistoryMessage::Ahead { count } => {
            text.literal("领先 ");
            text.argument(count);
        }
        RepoHistoryMessage::Behind { count } => {
            text.literal("落后 ");
            text.argument(count);
        }
        RepoHistoryMessage::OnlyNewest { count: commits } => {
            text.literal("仅列出最新的 ");
            text.number(count(commits));
            text.literal(" 个提交。");
        }
        RepoHistoryMessage::HighlightTrack { track } => {
            text.literal("高亮轨道 ");
            text.number(count(track));
        }
        RepoHistoryMessage::Track { track } => {
            text.literal("轨道 ");
            text.number(count(track));
        }
        RepoHistoryMessage::BranchName { name } => {
            text.literal("分支：");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count: branches } => {
            text.literal("另外 ");
            text.number(count(branches));
            text.literal(" 个分支");
        }
        RepoHistoryMessage::FilesChangedNoun { .. } => text.literal("个文件已更改"),
        RepoHistoryMessage::CompareTitle { base, head } => {
            text.literal("比较 ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
