//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/browse.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::RepoBrowseSentence;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};

/// A count as the English arm writes it.
fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

entries! { ZhHans:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("关于"),
        RepoBrowseLabel::NoDescription => text.literal("没有描述。"),
        RepoBrowseLabel::Visibility => text.literal("可见性"),
        RepoBrowseLabel::History => text.literal("历史记录"),
        RepoBrowseLabel::CommitHistory => text.literal("提交历史记录"),
        RepoBrowseLabel::AddFile => text.literal("添加文件"),
        RepoBrowseLabel::UploadFiles => text.literal("上传文件"),
        RepoBrowseLabel::Files => text.literal("文件"),
        RepoBrowseLabel::ParentDirectory => text.literal("上级目录"),
        RepoBrowseLabel::Path => text.literal("路径"),
        RepoBrowseLabel::Latest => text.literal("最新"),
        RepoBrowseLabel::YourAccess => text.literal("你的权限"),
        RepoBrowseLabel::LeaveRepository => text.literal("退出仓库"),
        RepoBrowseLabel::Languages => text.literal("语言"),
        RepoBrowseLabel::RepositoryLinks => text.literal("仓库链接"),
        RepoBrowseLabel::StoredWithLfs => text.literal("使用 Git LFS 存储"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("此服务器上没有该对象"),
        RepoBrowseLabel::EditThisFile => text.literal("编辑此文件"),
        RepoBrowseLabel::DeleteThisFile => text.literal("删除此文件"),
        RepoBrowseLabel::Blame => text.literal("追溯"),
        RepoBrowseLabel::Raw => text.literal("原始"),
        RepoBrowseLabel::Download => text.literal("下载"),
        RepoBrowseLabel::BinaryNotShown => text.literal("不显示二进制文件。"),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("此文件太大，无法显示。"),
        RepoBrowseLabel::ViewRaw => text.literal("查看原始文件"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("仓库文档"),
        RepoBrowseLabel::DocumentOutline => text.literal("文档大纲"),
        RepoBrowseLabel::FilterHeadings => text.literal("筛选标题"),
        RepoBrowseLabel::DocumentHeadings => text.literal("文档标题"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("没有匹配的标题。"),
        RepoBrowseLabel::EmptyTitle => text.literal("此仓库是空的"),
        RepoBrowseLabel::EmptyLead => text.literal("推送一个现有项目，或在命令行中新建一个项目。"),
        RepoBrowseLabel::RepositoryUrl => text.literal("仓库 URL"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("复制仓库 URL"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("SSH 仓库 URL"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("复制 SSH 仓库 URL"),
        RepoBrowseLabel::CreateNewRepository => text.literal("新建仓库"),
        RepoBrowseLabel::PushExistingRepository => text.literal("推送现有仓库"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("关闭文件浏览器"),
        RepoBrowseLabel::LanguageFiles => text.literal("语言文件"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("没有找到此语言的文件。"),
        RepoBrowseLabel::NoMatchingFiles => text.literal("没有匹配的文件。"),
        RepoBrowseLabel::FileContent => text.literal("文件内容"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("选择一个文件以预览其内容。"),
        RepoBrowseLabel::SearchThisListing => text.literal("搜索此列表"),
        RepoBrowseLabel::SearchNames => text.literal("搜索名称"),
        RepoBrowseLabel::Search => text.literal("搜索"),
        RepoBrowseLabel::ListingPages => text.literal("列表分页"),
        RepoBrowseLabel::Previous => text.literal("上一页"),
        RepoBrowseLabel::Next => text.literal("下一页"),
        RepoBrowseLabel::BranchesAndTags => text.literal("分支和标记"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("浏览分支和标记"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("查找分支或标记"),
        RepoBrowseLabel::Find => text.literal("查找"),
        RepoBrowseLabel::Branch => text.literal("分支"),
        RepoBrowseLabel::Tag => text.literal("标记"),
        RepoBrowseLabel::Default => text.literal("默认"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("没有匹配的分支或标记。"),
        RepoBrowseLabel::NoForksYet => text.literal("还没有复刻"),
        RepoBrowseLabel::ForksEmpty => text.literal("复刻是此仓库的副本，其他人可以在其中独立开展工作。"),
        RepoBrowseLabel::ForkLead => text.literal("复刻是你自己的一份仓库分支和标记副本。它沿用原仓库的可见性。"),
        RepoBrowseLabel::ForkLeadPrivate => {
            text.literal("复刻是你自己的一份仓库分支和标记副本。它沿用原仓库的可见性——这个复刻将保持私有。");
        }
        RepoBrowseLabel::Owner => text.literal("所有者"),
        RepoBrowseLabel::Name => text.literal("名称"),
        RepoBrowseLabel::CreateFork => text.literal("创建复刻"),
        RepoBrowseLabel::Cancel => text.literal("取消"),
        RepoBrowseLabel::NoStargazersYet => text.literal("还没有星标用户"),
        RepoBrowseLabel::StargazersEmpty => text.literal("为此仓库加星标的用户会显示在这里。"),
        RepoBrowseLabel::RepositoryActivity => text.literal("仓库动态"),
        RepoBrowseLabel::GoToFile => text.literal("转到文件"),
        RepoBrowseLabel::MatchingFiles => text.literal("匹配的文件"),
        RepoBrowseLabel::LoadingFiles => text.literal("正在加载文件…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("无法加载文件。按 Enter 键进行搜索。"),
        RepoBrowseLabel::SeeAllMatches => text.literal("查看全部匹配项"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("此修订中没有文件。"),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("此修订中的文件太多，无法全部搜索。"),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { .. } => text.literal("个分支"),
        RepoBrowseMessage::TagNoun { .. } => text.literal("个标记"),
        RepoBrowseMessage::CollaboratorAccess { permission } => {
            text.literal("作为协作者拥有");
            text.argument(permission);
            text.literal("权限。");
        }
        RepoBrowseMessage::LineCount { count: lines } => {
            text.number(count(lines));
            text.literal(" 行");
        }
        RepoBrowseMessage::LanguageFiles { language } => {
            text.argument(language);
            text.literal(" 文件");
        }
        RepoBrowseMessage::ListingResults { total, page, pages } => {
            text.number(count(total));
            text.literal(" 个结果 · ");
            text.number(count(page));
            text.literal(" / ");
            text.number(count(pages));
        }
        RepoBrowseMessage::RefPage { page, pages, total } => {
            text.number(count(page));
            text.literal(" / ");
            text.number(count(pages));
            text.literal(" · ");
            text.number(count(total));
            text.literal(" 个引用");
        }
        RepoBrowseMessage::ForkTitle { owner, name } => {
            text.literal("复刻 ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        RepoBrowseMessage::CommitNoun { .. } => text.literal("次提交"),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("没有与“");
            text.argument(query);
            text.literal("”匹配的文件。");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.literal("还有 ");
            text.number(count as u64);
            text.literal(" 个匹配项。输入更多文件名以缩小列表范围。");
        }
    }
    RepoBrowseSentence => RichWriter |text| {
        RepoBrowseSentence::EmptySshNote => {
            text.literal("下面的命令使用 HTTPS。如需通过 SSH 推送，请改用 SSH URL，并在以下位置添加密钥：");
            text.slot(0);
            text.literal("。");
        }
    }
}
